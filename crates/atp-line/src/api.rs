//! LINE Messaging API transport (`https://api.line.me/v2/bot/...`) with a
//! channel access token, from `LINE_CHANNEL_ACCESS_TOKEN` or the session
//! (`login --token`, kept as given). Errors (`{message, details}`) map onto
//! `atp_core::Error`; sends carry an `X-Line-Retry-Key` so a retried push
//! is not delivered twice, and the `X-Line-Request-Id` of each send comes
//! back (it identifies broadcasts in the message insights).

use std::time::Duration;

use atp_core::credentials;
use atp_core::http::{Method, Resp};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

pub const API: &str = "https://api.line.me";
pub const DATA: &str = "https://api-data.line.me";
const TOKEN_KEY: &str = "token";

pub struct Api {
  pub ctx: Ctx,
  env: Option<String>,
}

/// A random UUID (v4 layout) for `X-Line-Retry-Key`.
fn retry_key() -> String {
  let hex = format!("{:032x}", rand::random::<u128>());
  format!(
    "{}-{}-4{}-a{}-{}",
    &hex[..8],
    &hex[8..12],
    &hex[13..16],
    &hex[17..20],
    &hex[20..32]
  )
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env = credentials::env("LINE_CHANNEL_ACCESS_TOKEN");
    Self { ctx, env }
  }

  pub fn token(&self) -> Option<String> {
    self
      .env
      .clone()
      .or_else(|| self.ctx.extra(TOKEN_KEY))
      .filter(|t| !t.is_empty())
  }

  pub fn logged_in(&self) -> bool {
    self.token().is_some()
  }

  pub fn require(&self) -> Result<String> {
    self.token().ok_or_else(|| {
      Error::auth("no LINE channel access token")
        .with_hint("run `atp line login --token CHANNEL_ACCESS_TOKEN` (LINE Developers console → Messaging API)")
    })
  }

  pub fn save(&self, token: &str) {
    credentials::save(&self.ctx, &[(TOKEN_KEY, token.trim())]);
  }

  pub async fn get(&self, path: &str) -> Result<Value> {
    Ok(self.call(Method::GET, path, None, None).await?.0)
  }

  /// A send: JSON body, a retry key, and the request id that comes back.
  pub async fn send(&self, path: &str, body: &Value) -> Result<(Value, Option<String>)> {
    self
      .ctx
      .http
      .pause(Duration::from_millis(200), Duration::from_millis(600))
      .await;
    self
      .call(Method::POST, path, Some(body), Some(retry_key()))
      .await
  }

  pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
    Ok(self.call(Method::POST, path, Some(body), None).await?.0)
  }

  async fn call(
    &self,
    method: Method,
    path: &str,
    body: Option<&Value>,
    retry: Option<String>,
  ) -> Result<(Value, Option<String>)> {
    let token = self.require()?;
    let url = if path.starts_with("https://") {
      path.to_owned()
    } else {
      format!("{API}{path}")
    };
    let mut req = self
      .ctx
      .http
      .request(method, url)
      .no_cookies()
      .header("authorization", format!("Bearer {token}"));
    if let Some(k) = retry {
      req = req.header("x-line-retry-key", k).retries(2);
    }
    if let Some(b) = body {
      req = req.json(b);
    }
    let resp = req.send().await?;
    let request_id = resp.header("x-line-request-id").map(str::to_owned);
    Ok((check(resp, self.env.is_some())?, request_id))
  }
}

fn check(resp: Resp, from_env: bool) -> Result<Value> {
  let status = resp.status.as_u16();
  let v: Value = if resp.body.iter().all(u8::is_ascii_whitespace) {
    Value::Null
  } else {
    serde_json::from_slice(&resp.body).unwrap_or_default()
  };
  if resp.status.is_success() {
    return Ok(v);
  }
  // A retried send that already went through answers 409; that is success.
  if status == 409 && resp.header("x-line-accepted-request-id").is_some() {
    return Ok(Value::Null);
  }
  let mut text = v.str("message").unwrap_or_else(|| format!("HTTP {status}"));
  let details: Vec<String> = v
    .list("details")
    .iter()
    .filter_map(|d| {
      Some(format!(
        "{}: {}",
        d.str("property").unwrap_or_default(),
        d.str("message")?
      ))
    })
    .collect();
  if !details.is_empty() {
    text = format!("{text} ({})", details.join("; "));
  }
  Err(match status {
    401 => Error::auth(format!("LINE refused the channel access token: {text}")).with_hint(
      if from_env {
        "check LINE_CHANNEL_ACCESS_TOKEN"
      } else {
        "issue a new channel access token and log in again"
      },
    ),
    403 => Error::new(ErrorCode::PermissionDenied, text).with_hint(
      "member and follower lists need a verified or premium account; other calls need the right channel settings",
    ),
    404 => Error::not_found(text),
    429 => Error::new(ErrorCode::RateLimited, text),
    400 => Error::input(text),
    _ => Error::upstream(format!("LINE: {text} (HTTP {status})")),
  })
}
