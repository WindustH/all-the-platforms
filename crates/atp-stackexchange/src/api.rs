//! Stack Exchange API v2.3 transport: `GET
//! https://api.stackexchange.com/2.3/<method>` with the site, a filter that
//! names the fields wanted, and the app key and access token when there are
//! some; writes are form `POST`s.
//!
//! Answers come wrapped in `{items, has_more, quota_max, quota_remaining,
//! backoff}`; failures as `{error_id, error_name, error_message}`. A
//! `backoff` (seconds) has to pass before the next call: it is kept in the
//! cache, so the next run waits too. Without a key an IP may make 300
//! requests a day; with one, 10,000.

use std::cell::Cell;
use std::time::Duration;

use atp_core::http::{Resp, status_error};
use atp_core::output::note;
use atp_core::{Ctx, Error, ErrorCode, Page, PageReq, Result, Value, ValueExt, credentials, text};

pub const BASE: &str = "https://api.stackexchange.com/2.3";
pub const DEFAULT_SITE: &str = "stackoverflow";
/// Bodies, the answers and comments inside questions, comment bodies, links.
pub const FILTER_FULL: &str = "!9Nw.RAPWc)5LBmZcoUD6P-zmm8kEw1GuiHBQv0Yipf.UM-O*eoDCUNm";
/// The default fields plus post and answer titles, answer links, `about_me`
/// and inbox bodies.
pub const FILTER_LIST: &str = "!tmlrJq5LhTV1uEyxA6WpG7Ihj*ercmx";
/// Session extras.
pub const KEY: &str = "key";
pub const ACCESS_TOKEN: &str = "access_token";
pub const SITE: &str = "site";
pub const ENV_KEY: &str = "STACKEXCHANGE_KEY";
pub const ENV_TOKEN: &str = "STACKEXCHANGE_ACCESS_TOKEN";
pub const ENV_SITE: &str = "SE_SITE";
const PAGE_MAX: usize = 100;
/// Warn when fewer requests than this are left for the day.
const QUOTA_LOW: u64 = 20;
const BACKOFF_CACHE: &str = "backoff_until";
/// Longer backoffs fail instead of waiting.
const BACKOFF_WAIT_MAX: u64 = 30;

pub struct Api {
  pub ctx: Ctx,
  warned: Cell<bool>,
}

/// Request parameters.
pub type Params = Vec<(&'static str, String)>;

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      warned: Cell::new(false),
    }
  }

  fn saved(&self, env: &str, key: &str) -> Option<String> {
    credentials::env(env)
      .or_else(|| self.ctx.extra(key))
      .filter(|v| !v.is_empty())
  }

  pub fn key(&self) -> Option<String> {
    self.saved(ENV_KEY, KEY)
  }

  pub fn token(&self) -> Option<String> {
    self.saved(ENV_TOKEN, ACCESS_TOKEN)
  }

  /// `SE_SITE`, else the site chosen with `site`, else Stack Overflow.
  pub fn site(&self) -> String {
    self
      .saved(ENV_SITE, SITE)
      .unwrap_or_else(|| DEFAULT_SITE.to_owned())
  }

  /// The site a reference names, else the default one.
  pub fn site_or(&self, site: &Option<String>) -> String {
    site.clone().unwrap_or_else(|| self.site())
  }

  /// The key and access token that writes and the inbox need.
  pub fn require(&self) -> Result<(String, String)> {
    match (self.key(), self.token()) {
      (Some(key), Some(token)) => Ok((key, token)),
      _ => Err(
        Error::auth("this needs an access token of your own Stack Exchange app")
          .with_hint("run `atp so login --token 'CLIENT_ID:KEY'` (see `atp guide login so`)"),
      ),
    }
  }

  /// `GET <method>` on `site` (`None` for network-wide methods).
  pub async fn get(
    &self,
    method: &str,
    site: Option<&str>,
    mut params: Params,
    filter: &str,
  ) -> Result<Value> {
    self.wait_backoff().await?;
    params.push(("filter", filter.to_owned()));
    if let Some(site) = site {
      params.push(("site", site.to_owned()));
    }
    // A token goes only with the key it was issued for.
    if let Some(key) = self.key() {
      params.push(("key", key));
      if let Some(token) = self.token() {
        params.push(("access_token", token));
      }
    }
    let resp = self
      .ctx
      .http
      .get(format!("{BASE}/{method}"))
      .no_cookies()
      .queries(params)
      .send()
      .await?;
    self.check(resp)
  }

  /// A write on `site`: logged in, after a pause.
  pub async fn post(&self, method: &str, site: &str, mut form: Params) -> Result<Value> {
    let (key, token) = self.require()?;
    self.wait_backoff().await?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(1000), Duration::from_millis(2500))
      .await;
    form.extend([
      ("key", key),
      ("access_token", token),
      ("site", site.to_owned()),
      ("filter", FILTER_LIST.to_owned()),
    ]);
    let resp = self
      .ctx
      .http
      .post(format!("{BASE}/{method}"))
      .no_cookies()
      .form(form)
      .send()
      .await?;
    self.check(resp)
  }

  /// One page of a listing (`page` + `pagesize`), each item through `item`.
  pub async fn list<T>(
    &self,
    method: &str,
    site: Option<&str>,
    mut params: Params,
    filter: &str,
    page: &PageReq,
    item: impl Fn(&Value) -> T,
  ) -> Result<Page<T>> {
    let (no, size) = page_of(page, PAGE_MAX);
    params.push(("page", no.to_string()));
    params.push(("pagesize", size.to_string()));
    let v = self.get(method, site, params, filter).await?;
    let items = v.list("items").iter().map(item).collect();
    let next = (v.bool("has_more") == Some(true)).then(|| cursor(no + 1, size));
    Ok(Page::new(items, next))
  }

  fn check(&self, resp: Resp) -> Result<Value> {
    let Ok(v) = serde_json::from_slice::<Value>(&resp.body) else {
      return Err(status_error(resp.status, &resp.text()));
    };
    self.keep_backoff(&v);
    if let Some(id) = v.u64("error_id") {
      return Err(error(id, &v));
    }
    if !resp.status.is_success() {
      return Err(status_error(resp.status, &resp.text()));
    }
    self.watch_quota(&v);
    Ok(v)
  }

  fn keep_backoff(&self, v: &Value) {
    if let Some(secs) = v.u64("backoff").filter(|s| *s > 0) {
      let until = jiff::Timestamp::now().as_second() + secs as i64;
      self.ctx.store.cache_put(BACKOFF_CACHE, &until);
    }
  }

  /// Wait out a `backoff` an earlier answer asked for.
  async fn wait_backoff(&self) -> Result<()> {
    let year = Duration::from_secs(365 * 86_400);
    let Some(until) = self.ctx.store.cache_get::<i64>(BACKOFF_CACHE, year) else {
      return Ok(());
    };
    let left = until - jiff::Timestamp::now().as_second();
    if left <= 0 {
      return Ok(());
    }
    if left as u64 > BACKOFF_WAIT_MAX {
      return Err(Error::new(
        ErrorCode::RateLimited,
        format!("Stack Exchange asked to wait {left} more seconds before the next request"),
      ));
    }
    note(&format!(
      "Stack Exchange asked to back off; waiting {left} s"
    ));
    tokio::time::sleep(Duration::from_secs(left as u64)).await;
    Ok(())
  }

  fn watch_quota(&self, v: &Value) {
    let Some(left) = v.u64("quota_remaining") else {
      return;
    };
    if left < QUOTA_LOW && !self.warned.replace(true) {
      let max = v.u64("quota_max").unwrap_or_default();
      let hint = if self.key().is_none() {
        " (an app key raises the daily quota to 10,000: `atp guide login so`)"
      } else {
        ""
      };
      note(&format!(
        "Stack Exchange: {left} of {max} requests left today{hint}"
      ));
    }
  }
}

/// A Stack Exchange error (`error_id`) as an ATP error.
fn error(id: u64, v: &Value) -> Error {
  let name = v.str("error_name").unwrap_or_default();
  let message = text::html_to_text(&v.str("error_message").unwrap_or_default());
  let text = format!("{message} ({name})");
  match id {
    400 if message.contains("site") => {
      Error::input(text).with_hint("name a site from `atp so sites` (`SE_SITE` or `atp so site`)")
    }
    400 | 409 => Error::input(text),
    401 | 402 | 406 => Error::auth(text).with_hint("log in again: `atp guide login so`"),
    403 => Error::new(ErrorCode::PermissionDenied, text)
      .with_hint("the access token lacks this scope; log in again to grant it"),
    405 => Error::auth(text).with_hint("this method needs an app key: `atp guide login so`"),
    502 => Error::new(ErrorCode::RateLimited, text).with_hint(
      "the daily quota is used up or requests came too fast; an app key raises the quota",
    ),
    _ => Error::upstream(format!("Stack Exchange: {text}")),
  }
}

/// `(page, size)` of a request: the cursor carries both (`3:50`), so every
/// page of a listing has the size of the first.
pub fn page_of(page: &PageReq, max: usize) -> (u64, usize) {
  let parsed = page.cursor.as_deref().and_then(|cursor| {
    let (no, size) = cursor.split_once(':')?;
    Some((no.parse().ok()?, size.parse().ok()?))
  });
  parsed.unwrap_or_else(|| (page.number_or(1), page.size_within(max)))
}

pub fn cursor(no: u64, size: usize) -> String {
  format!("{no}:{size}")
}
