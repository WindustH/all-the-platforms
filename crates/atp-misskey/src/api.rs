//! Misskey transport: `POST https://<instance>/api/<endpoint>` with a JSON
//! body, the access token as its `i` field.
//!
//! The instance and token come from `MISSKEY_URL` / `MISSKEY_TOKEN` when set,
//! else from the session (`login --token INSTANCE|TOKEN` or MiAuth). Without
//! a token, requests go out anonymously to the instance (misskey.io unless
//! named), which serves public timelines, notes and users. Errors arrive as
//! `{"error": {code, message}}` and map onto `atp_core::Error`.

use std::path::Path;
use std::time::Duration;

pub use atp_core::credentials::host;
use atp_core::file::Upload;
use atp_core::http::Part;
use atp_core::{Ctx, Error, ErrorCode, Page, PageReq, Result, Value, ValueExt, credentials, json};

pub const DEFAULT_INSTANCE: &str = "misskey.io";
const INSTANCE_KEY: &str = "instance";
const TOKEN_KEY: &str = "token";
/// The most rows Misskey lists at once.
const PAGE_MAX: usize = 100;

pub struct Api {
  pub ctx: Ctx,
  env_instance: Option<String>,
  env_token: Option<String>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      env_instance: credentials::env("MISSKEY_URL").map(|url| host(&url)),
      env_token: credentials::env("MISSKEY_TOKEN"),
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
      .filter(|token| !token.is_empty())
  }

  pub fn logged_in(&self) -> bool {
    self.token().is_some()
  }

  pub fn require(&self) -> Result<String> {
    self.token().ok_or_else(|| {
      Error::auth(format!("not logged in to {}", self.instance())).with_hint(
        "run `atp misskey login` (MiAuth) or `atp misskey login --token 'INSTANCE|TOKEN'`",
      )
    })
  }

  pub fn save(&self, instance: &str, token: &str) {
    credentials::save(&self.ctx, &[(INSTANCE_KEY, instance), (TOKEN_KEY, token)]);
  }

  /// One endpoint, with the token when there is one.
  pub async fn call(&self, endpoint: &str, body: Value) -> Result<Value> {
    self.send(endpoint, with_token(body, self.token())).await
  }

  /// One endpoint that needs the account.
  pub async fn call_auth(&self, endpoint: &str, body: Value) -> Result<Value> {
    let token = self.require()?;
    self.send(endpoint, with_token(body, Some(token))).await
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

  /// One page of a `limit` + `untilId` listing: `body` plus the paging
  /// fields, each row through `item` (which gets the instance too). A full
  /// page has a next one, after the last row's `id`.
  pub async fn list<T>(
    &self,
    endpoint: &str,
    mut body: Value,
    page: &PageReq,
    item: impl Fn(&Value, &str) -> T,
  ) -> Result<Page<T>> {
    let size = page.size_within(PAGE_MAX);
    body["limit"] = json!(size);
    if let Some(cursor) = &page.cursor {
      body["untilId"] = json!(cursor);
    }
    let v = self.call(endpoint, body).await?;
    let rows = v.as_array().map(Vec::as_slice).unwrap_or_default();
    let instance = self.instance();
    let next = (rows.len() >= size)
      .then(|| rows.last().and_then(|row| row.str("id")))
      .flatten();
    Ok(Page::new(
      rows.iter().map(|row| item(row, &instance)).collect(),
      next,
    ))
  }

  /// One page of an `offset` listing (searches), each row through `item`.
  pub async fn list_by_offset<T>(
    &self,
    endpoint: &str,
    mut body: Value,
    page: &PageReq,
    item: impl Fn(&Value, &str) -> T,
  ) -> Result<Page<T>> {
    let size = page.size_within(PAGE_MAX);
    let offset: usize = page
      .cursor
      .as_deref()
      .and_then(|cursor| cursor.parse().ok())
      .unwrap_or(0);
    body["limit"] = json!(size);
    body["offset"] = json!(offset);
    let v = self.call(endpoint, body).await?;
    let rows = v.as_array().map(Vec::as_slice).unwrap_or_default();
    let instance = self.instance();
    let next = (rows.len() >= size).then(|| (offset + rows.len()).to_string());
    Ok(Page::new(
      rows.iter().map(|row| item(row, &instance)).collect(),
      next,
    ))
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
    check(resp.status.as_u16(), &resp.body)
  }

  /// Upload a file to the drive; its id goes into `fileIds`.
  pub async fn upload(&self, path: &Path) -> Result<String> {
    let token = self.require()?;
    let file = Upload::read(path).await?;
    let resp = self
      .ctx
      .http
      .post(format!("{}/api/drive/files/create", self.base()))
      .no_cookies()
      .multipart(vec![
        Part::text("i", token),
        Part::text("name", &file.name),
        Part::file("file", file.data, &file.name, file.mime),
      ])
      .timeout(Duration::from_secs(300))
      .send()
      .await?;
    let v = check(resp.status.as_u16(), &resp.body)?;
    v.str("id")
      .ok_or_else(|| Error::upstream("drive upload returned no file id"))
  }
}

fn with_token(mut body: Value, token: Option<String>) -> Value {
  if let (Some(token), Some(map)) = (token, body.as_object_mut()) {
    map.insert("i".into(), json!(token));
  }
  body
}

fn check(status: u16, body: &[u8]) -> Result<Value> {
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
      Error::auth(text).with_hint("this needs an account on the instance: `atp misskey login`")
    }
    (401, _) | (_, "AUTHENTICATION_FAILED") => {
      Error::auth(text).with_hint("the token was refused; log in again (`atp misskey login`)")
    }
    (_, "PERMISSION_DENIED") | (403, _) => Error::new(ErrorCode::PermissionDenied, text)
      .with_hint("the token may lack this permission; log in again to grant it"),
    (429, _) | (_, "RATE_LIMIT_EXCEEDED") => Error::new(ErrorCode::RateLimited, text),
    (_, code) if code.starts_with("NO_SUCH_") => Error::not_found(text),
    (400, _) => Error::input(text),
    _ => Error::upstream(format!("Misskey: {text}")),
  })
}
