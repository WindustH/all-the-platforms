//! V2EX transport: the open API 1.0 (`/api/<path>.json`, no account) and
//! API 2.0 (`/api/v2/<path>`, a personal access token as `Authorization:
//! Bearer`). API 2.0 answers `{success, message, result}`. V2EX allows an
//! IP 600 requests an hour (`X-Rate-Limit-*` headers). Requests name ATP
//! as their client: Cloudflare stops look-alike browsers with a challenge.

use std::cell::Cell;
use std::time::Duration;

use atp_core::http::{Method, Resp, TOOL_USER_AGENT, status_error};
use atp_core::output::note;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, credentials};

pub const BASE: &str = "https://www.v2ex.com";
pub const TOKEN: &str = "token";
pub const ENV_TOKEN: &str = "V2EX_TOKEN";
/// Warn when fewer requests than this are left in the hour.
const RATE_LOW: u64 = 30;

pub struct Api {
  pub ctx: Ctx,
  warned: Cell<bool>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      warned: Cell::new(false),
    }
  }

  pub fn token(&self) -> Option<String> {
    credentials::env(ENV_TOKEN)
      .or_else(|| self.ctx.extra(TOKEN))
      .filter(|token| !token.is_empty())
  }

  pub fn require(&self) -> Result<String> {
    self.token().ok_or_else(|| {
      Error::auth("this needs a V2EX personal access token")
        .with_hint("run `atp v2ex login --token TOKEN` (see `atp guide login v2ex`)")
    })
  }

  /// `GET /api/<path>` of API 1.0.
  pub async fn v1(&self, path: &str, params: Vec<(&str, String)>) -> Result<Value> {
    let resp = self
      .ctx
      .http
      .get(format!("{BASE}/api/{path}"))
      .no_cookies()
      .header("user-agent", TOOL_USER_AGENT)
      .queries(params)
      .send()
      .await?;
    let v = self.check(resp)?;
    if v.str("status").as_deref() == Some("error") {
      let message = v.str("message").unwrap_or_else(|| "error".into());
      return Err(Error::upstream(format!("V2EX: {message}")));
    }
    Ok(v)
  }

  /// `GET /api/v2/<path>` with the token: the whole answer (`result`,
  /// `pagination` …).
  pub async fn v2(&self, path: &str, params: Vec<(&str, String)>) -> Result<Value> {
    self.v2_call(Method::GET, path, params).await
  }

  /// `DELETE /api/v2/<path>`, after a short pause.
  pub async fn v2_delete(&self, path: &str) -> Result<Value> {
    self.require()?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(500), Duration::from_millis(1500))
      .await;
    self.v2_call(Method::DELETE, path, Vec::new()).await
  }

  async fn v2_call(
    &self,
    method: Method,
    path: &str,
    params: Vec<(&str, String)>,
  ) -> Result<Value> {
    let token = self.require()?;
    let resp = self
      .ctx
      .http
      .request(method, format!("{BASE}/api/v2/{path}"))
      .no_cookies()
      .header("user-agent", TOOL_USER_AGENT)
      .header("authorization", format!("Bearer {token}"))
      .queries(params)
      .send()
      .await?;
    let v = self.check(resp)?;
    if v.bool("success") == Some(false) {
      return Err(refused(&v.str("message").unwrap_or_default()));
    }
    Ok(v)
  }

  fn check(&self, resp: Resp) -> Result<Value> {
    self.watch_rate(&resp);
    let status = resp.status.as_u16();
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    let message = v.str("message").unwrap_or_default();
    if status == 403 && resp.text().contains("Just a moment") {
      return Err(
        Error::verification("V2EX's Cloudflare check stopped the request")
          .with_hint("retry later, or from another network")
          .unclearable(),
      );
    }
    match status {
      200..=299 => Ok(v),
      401 => Err(refused(&message)),
      403 | 429 if message.to_lowercase().contains("rate") || status == 429 => Err(
        Error::new(
          ErrorCode::RateLimited,
          format!(
            "V2EX: {}",
            if message.is_empty() {
              "rate limit reached"
            } else {
              &message
            }
          ),
        )
        .with_hint("V2EX allows 600 requests an hour per IP; wait for the next hour"),
      ),
      404 => Err(Error::not_found(format!(
        "V2EX: {}",
        if message.is_empty() {
          "not found"
        } else {
          &message
        }
      ))),
      _ => Err(status_error(resp.status, &resp.text())),
    }
  }

  fn watch_rate(&self, resp: &Resp) {
    let left = resp
      .header("x-rate-limit-remaining")
      .and_then(|v| v.parse::<u64>().ok());
    if let Some(left) = left
      && left < RATE_LOW
      && !self.warned.replace(true)
    {
      note(&format!("V2EX: {left} requests left this hour for this IP"));
    }
  }
}

/// A refusal of API 2.0 (`success: false` or 401).
fn refused(message: &str) -> Error {
  let lower = message.to_lowercase();
  if lower.contains("token") {
    return Error::auth(format!("V2EX: {message}"))
      .with_hint("check the token, or create a new one: `atp guide login v2ex`");
  }
  if lower.contains("not found") {
    return Error::not_found(format!("V2EX: {message}"));
  }
  Error::upstream(format!("V2EX: {message}"))
}
