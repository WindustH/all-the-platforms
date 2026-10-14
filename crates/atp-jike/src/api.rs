//! Jike transport: the REST API web.okjike.com calls
//! (`api.ruguoapp.com/1.0/…`), sent as the web app sends it: the access
//! token in `x-jike-access-token`, `platform: web`, JSON bodies. Listings
//! page with an opaque `loadMoreKey`, which ATP hands back as its cursor.
//!
//! Like the web app, ATP renews the access token with the refresh token
//! (`app_auth_tokens.refresh`, which also hands out a new refresh token) when
//! a call answers 401 or code `E101`, saves both, and repeats the call
//! once. Code `E202` comes with an alert Jike wants a human to read (risk
//! control): `verification_required` with the page it links to. HTTP 429
//! is `rate_limited`.
//!
//! Without a login, posts and their comments still read through the same
//! API, and users and topics through the server-rendered pages of
//! m.okjike.com ([`Api::page`]).

use std::time::Duration;

use atp_core::http::{Method, Resp, status_error};
use atp_core::json::next_data;
use atp_core::oauth::Tokens;
use atp_core::text::{one_line, truncate};
use atp_core::{Ctx, Error, ErrorCode, PageReq, Result, Value, ValueExt, json};

use crate::login;

pub const API: &str = "https://api.ruguoapp.com";
/// The web app; links point here.
pub const WEB: &str = "https://web.okjike.com";
/// The mobile share pages, server-rendered.
pub const MOBILE: &str = "https://m.okjike.com";
/// The most items one listing call asks for (the web app's own size).
const PAGE_MAX: usize = 20;

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  pub fn logged_in(&self) -> bool {
    Tokens::saved(&self.ctx).is_some()
  }

  pub fn require_login(&self) -> Result<()> {
    if self.logged_in() {
      return Ok(());
    }
    Err(Error::auth("not logged in to Jike").with_hint(self.ctx.login_hint()))
  }

  fn token(&self) -> Option<String> {
    Tokens::saved(&self.ctx).map(|t| t.access)
  }

  /// `GET <API><path>`.
  pub async fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
    self.call(Method::GET, path, params, None).await
  }

  /// `POST <API><path>` with a JSON body.
  pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
    self.call(Method::POST, path, &[], Some(&body)).await
  }

  /// `POST <API><path>?<params>` with an empty JSON body.
  pub async fn post_query(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
    self
      .call(Method::POST, path, params, Some(&json!({})))
      .await
  }

  /// A change to the account: needs a login and waits a few seconds first,
  /// as a person would between actions.
  pub async fn write(&self, path: &str, body: Value) -> Result<Value> {
    self.require_login()?;
    let http = &self.ctx.http;
    http
      .pause(Duration::from_secs(2), Duration::from_secs(6))
      .await;
    self.post(path, body).await
  }

  /// One page of a `{data, loadMoreKey}` listing: `body` plus the limit
  /// and the cursor (the last `loadMoreKey`, as JSON text).
  pub async fn list(
    &self,
    path: &str,
    mut body: Value,
    req: &PageReq,
  ) -> Result<(Value, Option<String>)> {
    body["limit"] = json!(req.size_within(PAGE_MAX));
    if let Some(cursor) = &req.cursor {
      body["loadMoreKey"] = serde_json::from_str(cursor).unwrap_or_else(|_| json!(cursor));
    }
    let v = self.post(path, body).await?;
    let next = v
      .get("loadMoreKey")
      .filter(|key| !key.is_null() && !v.list("data").is_empty())
      .map(Value::to_string);
    Ok((v, next))
  }

  async fn call(
    &self,
    method: Method,
    path: &str,
    params: &[(&str, String)],
    body: Option<&Value>,
  ) -> Result<Value> {
    let token = self.token();
    let resp = self
      .send(method.clone(), path, params, body, token.as_deref())
      .await?;
    if token.is_some() && expired(&resp) {
      let fresh = self.renew().await?;
      let again = self.send(method, path, params, body, Some(&fresh)).await?;
      return self.check(again);
    }
    self.check(resp)
  }

  async fn send(
    &self,
    method: Method,
    path: &str,
    params: &[(&str, String)],
    body: Option<&Value>,
    token: Option<&str>,
  ) -> Result<Resp> {
    let url = if path.starts_with("https://") {
      path.to_owned()
    } else {
      format!("{API}{path}")
    };
    let mut req = self
      .ctx
      .http
      .request(method, url)
      .queries(params.iter().map(|(k, v)| (*k, v)))
      .header("accept", "application/json, text/plain, */*")
      .header("origin", WEB)
      .header("platform", "web");
    if let Some(token) = token {
      req = req.header("x-jike-access-token", token);
    }
    if let Some(body) = body {
      req = req.json(body);
    }
    req.send().await
  }

  /// A fresh access token from the saved refresh token, saved at once with
  /// the new refresh token Jike answers with, which replaces the old one.
  async fn renew(&self) -> Result<String> {
    let refresh = Tokens::saved(&self.ctx)
      .and_then(|t| t.refresh)
      .ok_or_else(|| Error::auth("the Jike session ended").with_hint(self.ctx.login_hint()))?;
    let renewed = login::refresh(self, &refresh)
      .await
      .map_err(|e| match e.code {
        ErrorCode::NotAuthenticated | ErrorCode::InvalidInput | ErrorCode::PermissionDenied => {
          Error::auth(format!(
            "the Jike session could not be renewed: {}",
            e.message
          ))
          .with_hint(self.ctx.login_hint())
        }
        _ => e,
      })?;
    renewed.save(&self.ctx)?;
    Ok(renewed.access)
  }

  /// The JSON answer, or the error it stands for.
  fn check(&self, resp: Resp) -> Result<Value> {
    let v: Option<Value> = serde_json::from_slice(&resp.body).ok();
    let Some(v) = v else {
      if resp.status.as_u16() == 401 {
        let what = if self.logged_in() {
          "the Jike session ended"
        } else {
          "Jike answers this only to a logged-in account"
        };
        return Err(Error::auth(what).with_hint(self.ctx.login_hint()));
      }
      if !resp.status.is_success() {
        return Err(status_error(resp.status, &resp.text()));
      }
      return Err(Error::upstream(format!(
        "Jike answered with a page instead of data: {}",
        truncate(&one_line(&resp.text()), 160)
      )));
    };
    let failed =
      v.bool("success") == Some(false) || v.str("code").is_some_and(|code| code.starts_with('E'));
    if resp.status.is_success() && !failed {
      return Ok(v);
    }
    Err(self.error(resp.status.as_u16(), &v))
  }

  /// Jike's error answer (`{code, error, toast, alert}`) as a shared error.
  fn error(&self, status: u16, v: &Value) -> Error {
    let code = v.str("code").unwrap_or_default();
    let message = v
      .first_str(&["error", "toast", "message", "alert.title"])
      .unwrap_or_else(|| format!("HTTP {status}"));
    let text = format!("Jike: {message}");
    if status == 401 || code == "E101" {
      return Error::auth(text).with_hint(self.ctx.login_hint());
    }
    if code == "E202" {
      return verification(v, text);
    }
    match status {
      429 => Error::new(ErrorCode::RateLimited, text).with_hint("wait a while before retrying"),
      404 => Error::not_found(text),
      403 => Error::new(ErrorCode::PermissionDenied, text),
      400 if gone(&message) => Error::not_found(text),
      400 => Error::input(text),
      _ if gone(&message) => Error::not_found(text),
      _ => Error::upstream(text),
    }
  }

  /// The `pageProps` of a server-rendered page of m.okjike.com.
  pub async fn page(&self, path: &str) -> Result<Value> {
    let url = format!("{MOBILE}{path}");
    let resp = self
      .ctx
      .http
      .get(&url)
      .no_cookies()
      .header("accept", "text/html,application/xhtml+xml")
      .send()
      .await?;
    if resp.status.as_u16() == 404 {
      return Err(Error::not_found(format!("Jike has no page {url}")));
    }
    let resp = resp.check()?;
    let data = next_data(&resp.text())
      .ok_or_else(|| Error::upstream(format!("Jike's page {url} carried no data")))?;
    Ok(data.at("props.pageProps").clone())
  }
}

/// Whether an answer asks for a fresh access token.
fn expired(resp: &Resp) -> bool {
  if resp.status.as_u16() == 401 {
    return true;
  }
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
  v.str("code").as_deref() == Some("E101")
}

/// Messages of content that is gone or hidden.
fn gone(message: &str) -> bool {
  ["不存在", "已删除", "已注销", "无法访问"]
    .iter()
    .any(|w| message.contains(w))
}

/// An `E202` alert: the page it links to, or the web app.
fn verification(v: &Value, text: String) -> Error {
  let page = v
    .str("alert.confirmButton.linkUrl")
    .filter(|url| url.starts_with("http"))
    .unwrap_or_else(|| WEB.to_owned());
  let detail = v.str("alert.description").unwrap_or_default();
  Error::new(
    ErrorCode::VerificationRequired,
    format!("{text} {detail}").trim().to_owned(),
  )
  .with_url(page)
  .with_hint("open the page, do what Jike asks there, then retry; slow down with `--interval`")
}
