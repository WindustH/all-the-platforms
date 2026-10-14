//! Douban transport. Reads go to the JSON API the mobile site's own pages
//! call (`m.douban.com/rexxar/api/v2`, no app signature), to the mobile
//! site's `/j/` routes and, for the new-film chart, to a `movie.douban.com`
//! page. Every call names the page it is made from (`Referer`), carries
//! the `ck` token as the pages do (empty without a login) and the `bid`
//! browser cookie Douban hands every visitor: without it group topics
//! answer `content_view_permission`.
//!
//! Douban's checks (`sec.douban.com`, captcha and spam answers) become
//! `verification_required` with the page to open.

use std::time::Duration;

use atp_core::http::{Method, Resp};
use atp_core::text::{one_line, truncate};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

/// The mobile site, whose pages make the calls.
pub const MOBILE: &str = "https://m.douban.com";
pub const REXXAR: &str = "https://m.douban.com/rexxar/api/v2";
pub const ACCOUNTS: &str = "https://accounts.douban.com";
/// Cookies of a logged-in session.
pub const LOGIN_COOKIES: &[&str] = &["dbcl2"];
/// The token writes carry.
const CK: &str = "ck";

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  pub fn require_login(&self) -> Result<()> {
    self.ctx.require_login(LOGIN_COOKIES)
  }

  /// The `ck` token of the session (empty without a login).
  pub fn ck(&self) -> String {
    self
      .ctx
      .http
      .cookie(CK)
      .map(|ck| ck.trim_matches('"').to_owned())
      .unwrap_or_default()
  }

  /// The account's numeric id, from `dbcl2` (`"<id>:<secret>"`).
  pub fn my_id(&self) -> Option<String> {
    let cookie = self.ctx.http.cookie("dbcl2")?;
    let id = cookie.trim_matches('"').split(':').next()?.to_owned();
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then_some(id)
  }

  /// The `bid` cookie every Douban page sets for a new browser.
  async fn ensure_bid(&self) -> Result<()> {
    if self.ctx.http.has_cookie("bid") {
      return Ok(());
    }
    self
      .ctx
      .http
      .get(format!("{MOBILE}/"))
      .header("accept", "text/html,application/xhtml+xml")
      .send()
      .await?;
    Ok(())
  }

  /// `GET <REXXAR><path>` as the mobile page `page` (a path) makes it.
  pub async fn get(&self, path: &str, page: &str, params: &[(&str, String)]) -> Result<Value> {
    let mut params = params.to_vec();
    params.push(("for_mobile", "1".into()));
    self.call(&format!("{REXXAR}{path}"), page, &params).await
  }

  /// `GET` of one of the mobile site's own JSON routes (`/j/…`).
  pub async fn mobile(&self, path: &str, page: &str, params: &[(&str, String)]) -> Result<Value> {
    self.call(&format!("{MOBILE}{path}"), page, params).await
  }

  async fn call(&self, url: &str, page: &str, params: &[(&str, String)]) -> Result<Value> {
    self.ensure_bid().await?;
    let resp = self
      .ctx
      .http
      .get(url)
      .queries(params.iter().map(|(k, v)| (*k, v)))
      .query(CK, self.ck())
      .header("referer", format!("{MOBILE}{page}"))
      .header("x-requested-with", "XMLHttpRequest")
      .send()
      .await?;
    self.check(resp, page)
  }

  /// A page of a Douban site, as HTML (the film charts).
  pub async fn html(&self, url: &str) -> Result<String> {
    self.ensure_bid().await?;
    let resp = self
      .ctx
      .http
      .get(url)
      .header("accept", "text/html,application/xhtml+xml")
      .send()
      .await?;
    if let Some(check) = checkpoint(&resp) {
      return Err(verification(check));
    }
    Ok(resp.check()?.text())
  }

  /// A change to the account, sent as the mobile page's form posts are:
  /// needs `ck`, and waits a few seconds first (Douban bans accounts that
  /// write in bursts).
  pub async fn post(&self, path: &str, page: &str, form: &[(&str, String)]) -> Result<Value> {
    self.require_login()?;
    let ck = self.ck();
    if ck.is_empty() {
      return Err(
        Error::auth("the session has no `ck` cookie, which Douban's forms need")
          .with_hint(self.ctx.login_hint()),
      );
    }
    let http = &self.ctx.http;
    http
      .pause(Duration::from_secs(3), Duration::from_secs(8))
      .await;
    let mut form = form.to_vec();
    form.push((CK, ck));
    let resp = http
      .request(Method::POST, format!("{REXXAR}{path}"))
      .form(form)
      .header("referer", format!("{MOBILE}{page}"))
      .header("origin", MOBILE)
      .send()
      .await?;
    self.check(resp, page)
  }

  /// The JSON answer, or the error it stands for.
  fn check(&self, resp: Resp, page: &str) -> Result<Value> {
    if let Some(check) = checkpoint(&resp) {
      return Err(verification(check));
    }
    let v: Option<Value> = serde_json::from_slice(&resp.body).ok();
    let status = resp.status.as_u16();
    match v {
      Some(v) if resp.status.is_success() => Ok(v),
      Some(v) => Err(self.error(status, &v, page)),
      None if resp.status.is_success() => Err(Error::upstream(format!(
        "Douban answered with a page instead of data: {}",
        truncate(&one_line(&resp.text()), 160)
      ))),
      None => Err(atp_core::http::status_error(resp.status, &resp.text())),
    }
  }

  /// Douban's error answer (`{msg, code, localized_message}`) as a shared error.
  fn error(&self, status: u16, v: &Value, page: &str) -> Error {
    let msg = v.str("msg").unwrap_or_default();
    let code = v.i64("code").unwrap_or_default();
    let text = v
      .first_str(&["localized_message", "msg"])
      .unwrap_or_else(|| format!("HTTP {status}"));
    let text = format!("Douban: {text} ({msg})");
    let has = |words: &[&str]| words.iter().any(|w| msg.contains(w));
    if status == 401 || matches!(code, 103 | 1000) || has(&["need_login", "invalid_token"]) {
      return Error::auth(text).with_hint(self.ctx.login_hint());
    }
    if status == 429 || has(&["rate_limit"]) {
      return Error::new(ErrorCode::RateLimited, text).with_hint("wait a while before retrying");
    }
    if code == 1108 && v.str("extra.field").is_some_and(|f| f.contains("spam_id")) {
      return Error::new(ErrorCode::PermissionDenied, text).with_hint(
        "Douban serves a group topic only through the link its lists give (with `_spm_id`): \
         list the group with `atp douban group GROUP`, then use `#N` or that link",
      );
    }
    if code == 1108 || has(&["captcha", "spam", "verify", "abnormal"]) {
      return Error::new(ErrorCode::VerificationRequired, text)
        .with_url(format!("{MOBILE}{page}"))
        .with_hint("open the page in a browser and pass Douban's check, then retry");
    }
    match status {
      404 => Error::not_found(text),
      403 => Error::new(ErrorCode::PermissionDenied, text),
      400 => Error::input(text),
      _ => Error::upstream(text),
    }
  }
}

/// The address of Douban's human check when the answer is one.
fn checkpoint(resp: &Resp) -> Option<String> {
  if resp.url.contains("sec.douban.com") || resp.url.contains("/misc/sorry") {
    return Some(resp.url.clone());
  }
  let head = String::from_utf8_lossy(&resp.body[..resp.body.len().min(4096)]).into_owned();
  let start = head.find("https://sec.douban.com")?;
  let link: String = head[start..]
    .chars()
    .take_while(|c| !matches!(c, '"' | '\'' | '<' | ' '))
    .collect();
  Some(link)
}

fn verification(url: String) -> Error {
  Error::new(
    ErrorCode::VerificationRequired,
    "Douban wants a human check (too many requests from this address or account)",
  )
  .with_url(url)
  .with_hint("pass the check in a browser, then retry; slow down with `--interval`")
}
