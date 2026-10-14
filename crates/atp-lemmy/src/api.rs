//! Lemmy transport: the HTTP API v3 of one instance
//! (`https://<instance>/api/v3/<endpoint>`), reads as `GET` with query
//! parameters, writes as `POST` / `PUT` with JSON bodies, the login token
//! (a JWT) as `Authorization: Bearer`.
//!
//! The instance and token come from `LEMMY_INSTANCE` / `LEMMY_TOKEN` when
//! set, else from the session (`login --token`), else lemmy.world without a
//! token. Errors arrive as `{"error": "couldnt_find_post"}` and map onto
//! `atp_core::Error`.

use std::time::Duration;

use atp_core::http::{Method, TOOL_USER_AGENT, status_error};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, credentials};

pub const DEFAULT_INSTANCE: &str = "lemmy.world";
pub const INSTANCE_KEY: &str = "instance";
pub const TOKEN_KEY: &str = "token";
pub const ENV_INSTANCE: &str = "LEMMY_INSTANCE";
pub const ENV_TOKEN: &str = "LEMMY_TOKEN";
pub const ENV_TOTP: &str = "LEMMY_TOTP";

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  /// The instance's host (`lemmy.world`).
  pub fn instance(&self) -> String {
    credentials::env(ENV_INSTANCE)
      .map(|url| host(&url))
      .or_else(|| self.ctx.extra(INSTANCE_KEY))
      .filter(|host| !host.is_empty())
      .unwrap_or_else(|| DEFAULT_INSTANCE.to_owned())
  }

  pub fn base(&self) -> String {
    format!("https://{}", self.instance())
  }

  pub fn token(&self) -> Option<String> {
    credentials::env(ENV_TOKEN)
      .or_else(|| self.ctx.extra(TOKEN_KEY))
      .filter(|token| !token.is_empty())
  }

  pub fn require(&self) -> Result<String> {
    self.token().ok_or_else(|| {
      Error::auth(format!("not logged in to {}", self.instance())).with_hint(
        "run `atp lemmy login --token 'INSTANCE|USER|PASSWORD'` (see `atp guide login lemmy`)",
      )
    })
  }

  /// `GET /api/v3/<endpoint>`, with the token when there is one.
  pub async fn get(&self, endpoint: &str, params: Vec<(&str, String)>) -> Result<Value> {
    let url = format!("{}/api/v3/{endpoint}", self.base());
    self
      .call(Method::GET, url, params, None, self.token())
      .await
  }

  /// A change: logged in, after a short pause, as `POST` with a JSON body.
  pub async fn write(&self, endpoint: &str, body: Value) -> Result<Value> {
    self.require()?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(400), Duration::from_millis(1200))
      .await;
    let url = format!("{}/api/v3/{endpoint}", self.base());
    self
      .call(Method::POST, url, Vec::new(), Some(body), self.token())
      .await
  }

  /// `POST` to an instance named outright, without a token (logging in).
  pub async fn post_to(&self, instance: &str, endpoint: &str, body: Value) -> Result<Value> {
    let url = format!("https://{instance}/api/v3/{endpoint}");
    self
      .call(Method::POST, url, Vec::new(), Some(body), None)
      .await
  }

  async fn call(
    &self,
    method: Method,
    url: String,
    params: Vec<(&str, String)>,
    body: Option<Value>,
    token: Option<String>,
  ) -> Result<Value> {
    let mut req = self
      .ctx
      .http
      .request(method, url)
      .no_cookies()
      .header("user-agent", TOOL_USER_AGENT)
      .queries(params);
    if let Some(token) = token {
      req = req.header("authorization", format!("Bearer {token}"));
    }
    if let Some(body) = &body {
      req = req.json(body);
    }
    let resp = req.send().await?;
    let status = resp.status.as_u16();
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    match v.str("error") {
      Some(code) => Err(refused(&code)),
      None if (200..300).contains(&status) => Ok(v),
      None => Err(status_error(resp.status, &resp.text())),
    }
  }
}

/// An `{"error": code}` answer as an error.
fn refused(code: &str) -> Error {
  let text = format!("Lemmy: {code}");
  match code {
    "not_logged_in" | "incorrect_login" | "invalid_auth_token" | "not_logged_in_or_banned" => {
      Error::auth(text).with_hint("log in again: `atp guide login lemmy`")
    }
    "missing_totp_token" | "incorrect_totp_token" => Error::auth(text).with_hint(
      "the account uses two-factor login: set LEMMY_TOTP to the current code and log in again",
    ),
    "rate_limit_error" => Error::new(ErrorCode::RateLimited, text),
    code if code.starts_with("couldnt_find") || code.ends_with("not_found") => {
      Error::not_found(text)
    }
    code if code.contains("banned") || code.starts_with("no_") || code.contains("permission") => {
      Error::new(ErrorCode::PermissionDenied, text)
    }
    _ => Error::upstream(text),
  }
}

/// `https://lemmy.world/` or `lemmy.world` → `lemmy.world`.
pub fn host(url: &str) -> String {
  let url = url.trim();
  let url = url
    .strip_prefix("https://")
    .or_else(|| url.strip_prefix("http://"))
    .unwrap_or(url);
  url.split('/').next().unwrap_or(url).to_ascii_lowercase()
}
