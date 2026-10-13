//! Misskey transport: `POST https://<instance>/api/<endpoint>` with a JSON
//! body, the access token as its `i` field.
//!
//! The instance and token come from `MISSKEY_URL` / `MISSKEY_TOKEN` when set,
//! else from the session (`login --token INSTANCE|TOKEN` or MiAuth). Without
//! a token, requests go out anonymously to the instance (misskey.io unless
//! named), which serves public timelines, notes and users. Errors arrive as
//! `{"error": {code, message}}` and map onto `social_core::Error`.

use std::time::Duration;

use social_core::http::Part;
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

pub const DEFAULT_INSTANCE: &str = "misskey.io";
const INSTANCE_KEY: &str = "instance";
const TOKEN_KEY: &str = "token";

pub struct Api {
  pub ctx: Ctx,
  env_instance: Option<String>,
  env_token: Option<String>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    Self {
      ctx,
      env_instance: var("MISSKEY_URL").map(|u| host(&u)),
      env_token: var("MISSKEY_TOKEN"),
    }
  }

  /// The instance's host (`misskey.io`).
  pub fn instance(&self) -> String {
    self
      .env_instance
      .clone()
      .or_else(|| self.ctx.extra(INSTANCE_KEY))
      .unwrap_or_else(|| DEFAULT_INSTANCE.to_owned())
  }

  pub fn base(&self) -> String {
    format!("https://{}", self.instance())
  }

  pub fn token(&self) -> Option<String> {
    self
      .env_token
      .clone()
      .or_else(|| self.ctx.extra(TOKEN_KEY))
      .filter(|t| !t.is_empty())
  }

  pub fn logged_in(&self) -> bool {
    self.token().is_some()
  }

  pub fn require(&self) -> Result<String> {
    self.token().ok_or_else(|| {
      Error::auth(format!("not logged in to {}", self.instance())).with_hint(
        "run `social misskey login` (MiAuth) or `social misskey login --token 'INSTANCE|TOKEN'`",
      )
    })
  }

  pub fn save(&self, instance: &str, token: &str) {
    self.ctx.set_extra(INSTANCE_KEY, instance);
    self.ctx.set_extra(TOKEN_KEY, token);
  }

  /// One endpoint, with the token when there is one.
  pub async fn call(&self, endpoint: &str, body: Value) -> Result<Value> {
    let body = self.with_token(body, self.token());
    self.send(endpoint, body).await
  }

  /// One endpoint that needs the account.
  pub async fn call_auth(&self, endpoint: &str, body: Value) -> Result<Value> {
    let token = self.require()?;
    let body = self.with_token(body, Some(token));
    self.send(endpoint, body).await
  }

  /// A write: logged in, after a short pause.
  pub async fn write(&self, endpoint: &str, body: Value) -> Result<Value> {
    self.require()?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(300), Duration::from_millis(900))
      .await;
    self.call_auth(endpoint, body).await
  }

  fn with_token(&self, mut body: Value, token: Option<String>) -> Value {
    if let (Some(t), Some(map)) = (token, body.as_object_mut()) {
      map.insert("i".into(), json!(t));
    }
    body
  }

  async fn send(&self, endpoint: &str, body: Value) -> Result<Value> {
    let resp = self
      .ctx
      .http
      .post(format!("{}/api/{endpoint}", self.base()))
      .no_cookies()
      .json(&body)
      .send()
      .await?;
    self.check(resp.status.as_u16(), &resp.body)
  }

  /// Upload a file to the drive; its id goes into `fileIds`.
  pub async fn upload(&self, name: &str, data: Vec<u8>, mime: &str) -> Result<String> {
    let token = self.require()?;
    let resp = self
      .ctx
      .http
      .post(format!("{}/api/drive/files/create", self.base()))
      .no_cookies()
      .multipart(vec![
        Part::text("i", token),
        Part::text("name", name),
        Part::file("file", data, name, mime),
      ])
      .timeout(Duration::from_secs(300))
      .send()
      .await?;
    let v = self.check(resp.status.as_u16(), &resp.body)?;
    v.str("id")
      .ok_or_else(|| Error::upstream("drive upload returned no file id"))
  }

  fn check(&self, status: u16, body: &[u8]) -> Result<Value> {
    if (200..300).contains(&status) {
      if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Null);
      }
      return Ok(serde_json::from_slice(body)?);
    }
    let v: Value = serde_json::from_slice(body).unwrap_or_default();
    let code = v.str("error.code").unwrap_or_default();
    let message = v
      .str("error.message")
      .unwrap_or_else(|| format!("HTTP {status}"));
    let text = if code.is_empty() {
      message
    } else {
      format!("{message} ({code})")
    };
    Err(match (status, code.as_str()) {
      (_, "CREDENTIAL_REQUIRED") => {
        Error::auth(text).with_hint("this needs an account on the instance: `social misskey login`")
      }
      (401, _) | (_, "AUTHENTICATION_FAILED") => {
        Error::auth(text).with_hint("the token was refused; log in again (`social misskey login`)")
      }
      (_, "PERMISSION_DENIED") | (403, _) => Error::new(ErrorCode::PermissionDenied, text)
        .with_hint("the token may lack this permission; log in again to grant it"),
      (429, _) | (_, "RATE_LIMIT_EXCEEDED") => Error::new(ErrorCode::RateLimited, text),
      (_, c) if c.starts_with("NO_SUCH_") => Error::not_found(text),
      (400, _) => Error::input(text),
      _ => Error::upstream(format!("Misskey: {text}")),
    })
  }
}

/// `https://misskey.io/` or `misskey.io` → `misskey.io`.
pub fn host(s: &str) -> String {
  let s = s.trim();
  let s = s
    .strip_prefix("https://")
    .or_else(|| s.strip_prefix("http://"))
    .unwrap_or(s);
  s.split('/').next().unwrap_or(s).to_ascii_lowercase()
}
