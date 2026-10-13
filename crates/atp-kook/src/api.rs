//! KOOK transport: `https://www.kookapp.cn/api/v3` with a bot token
//! (`Authorization: Bot …`). The token comes from `KOOK_TOKEN` when set,
//! else from `login --token`. Replies are `{code, message, data}`; list
//! endpoints page by number (`data.meta.page_total`). A 429 waits for
//! `X-Rate-Limit-Reset` and retries.

use std::time::Duration;

use atp_core::credentials;
use atp_core::http::{Method, Part, Resp};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

pub const API: &str = "https://www.kookapp.cn/api/v3";
const TOKEN_KEY: &str = "token";
const MAX_TRIES: u32 = 3;

pub struct Api {
  pub ctx: Ctx,
  env: Option<String>,
}

enum Body {
  None,
  Json(Value),
  Multipart(Vec<Part>),
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env = credentials::env("KOOK_TOKEN");
    Self { ctx, env }
  }

  pub fn token(&self) -> Option<String> {
    self
      .env
      .clone()
      .or_else(|| self.ctx.extra(TOKEN_KEY).filter(|t| !t.is_empty()))
  }

  pub fn logged_in(&self) -> bool {
    self.token().is_some()
  }

  pub fn save_token(&self, token: &str) {
    self.ctx.set_extra(TOKEN_KEY, token);
  }

  fn require(&self) -> Result<String> {
    self.token().ok_or_else(|| {
      Error::auth("no KOOK bot token").with_hint(
        "create a bot at https://developer.kookapp.cn/app, then run \
         `atp kook login --token '...'` or set KOOK_TOKEN",
      )
    })
  }

  /// `Bot <token>` unless the token names its scheme already.
  pub fn authorization(token: &str) -> String {
    let t = token.trim();
    if t.starts_with("Bot ") || t.starts_with("Bearer ") {
      t.to_owned()
    } else {
      format!("Bot {t}")
    }
  }

  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let token = self.require()?;
    self
      .send(&token, Method::GET, path, query, Body::None)
      .await
  }

  pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
    let token = self.require()?;
    self
      .send(&token, Method::POST, path, &[], Body::Json(body))
      .await
  }

  pub async fn upload(&self, parts: Vec<Part>) -> Result<Value> {
    let token = self.require()?;
    self
      .send(
        &token,
        Method::POST,
        "/asset/create",
        &[],
        Body::Multipart(parts),
      )
      .await
  }

  /// `GET /user/me` with a token that is not saved yet.
  pub async fn me_with(&self, token: &str) -> Result<Value> {
    self
      .send(token, Method::GET, "/user/me", &[], Body::None)
      .await
  }

  async fn send(
    &self,
    token: &str,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Body,
  ) -> Result<Value> {
    let mut tries = 0;
    loop {
      tries += 1;
      let mut req = self
        .ctx
        .http
        .request(method.clone(), format!("{API}{path}"))
        .no_cookies()
        .header("authorization", Self::authorization(token))
        .queries(query.iter().map(|(k, v)| (*k, v)));
      req = match &body {
        Body::None => req,
        Body::Json(v) => req.json(v),
        Body::Multipart(parts) => req.multipart(parts.clone()),
      };
      let resp = req.send().await?;
      if resp.status.as_u16() == 429 && tries < MAX_TRIES {
        tokio::time::sleep(retry_after(&resp)).await;
        continue;
      }
      return outcome(&resp);
    }
  }
}

/// How long a 429 asks to wait (`X-Rate-Limit-Reset`, in seconds).
fn retry_after(resp: &Resp) -> Duration {
  let secs = resp
    .header("x-rate-limit-reset")
    .and_then(|s| s.parse::<f64>().ok())
    .unwrap_or(1.0)
    .clamp(0.5, 30.0);
  Duration::from_secs_f64(secs)
}

/// `data` of a `{code: 0, data}` reply; anything else as an error.
fn outcome(resp: &Resp) -> Result<Value> {
  let status = resp.status.as_u16();
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
  let code = v.i64("code");
  if status == 401 || code == Some(401) {
    return Err(
      Error::auth("KOOK did not accept the token")
        .with_hint("check the bot token at https://developer.kookapp.cn/app"),
    );
  }
  if status == 429 {
    return Err(Error::new(
      ErrorCode::RateLimited,
      "KOOK rate limit reached",
    ));
  }
  if code == Some(0) {
    return Ok(v.at("data").clone());
  }
  let message = v.str("message").unwrap_or_else(|| format!("HTTP {status}"));
  Err(match status {
    403 => Error::new(ErrorCode::PermissionDenied, format!("KOOK: {message}")),
    404 => Error::not_found(format!("KOOK: {message}")),
    _ => Error::upstream(format!("KOOK: {message}")),
  })
}
