//! Bangumi transport: the official API at `api.bgm.tv` (v0, plus the older
//! `/calendar`). Requests carry the User-Agent Bangumi's API rules ask
//! for (the client's name and where it lives); with a personal access
//! token they also carry `Authorization: Bearer`. Errors arrive as
//! `{title, description, details}`.

use std::time::Duration;

use atp_core::http::{Method, Resp, status_error};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, credentials};

pub const BASE: &str = "https://api.bgm.tv";
/// The web site, for links.
pub const SITE: &str = "https://bgm.tv";
/// What Bangumi asks clients to send: `<owner>/<project> (<home page>)`.
pub const USER_AGENT: &str = "WindustH/atp (https://github.com/WindustH/atp)";
pub const TOKEN: &str = "token";
pub const ENV_TOKEN: &str = "BANGUMI_TOKEN";

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  pub fn token(&self) -> Option<String> {
    credentials::env(ENV_TOKEN)
      .or_else(|| self.ctx.extra(TOKEN))
      .filter(|token| !token.is_empty())
  }

  pub fn require(&self) -> Result<String> {
    self.token().ok_or_else(|| {
      Error::auth("this needs a Bangumi access token")
        .with_hint("run `atp bangumi login --token TOKEN` (see `atp guide login bangumi`)")
    })
  }

  /// `GET <BASE><path>` (with the token when there is one).
  pub async fn get(&self, path: &str, params: Vec<(&str, String)>) -> Result<Value> {
    self.call(Method::GET, path, params, None).await
  }

  /// `POST <BASE><path>` with a JSON body (searches).
  pub async fn post(&self, path: &str, params: Vec<(&str, String)>, body: Value) -> Result<Value> {
    self.call(Method::POST, path, params, Some(body)).await
  }

  /// A change to the account: needs the token, after a short pause.
  pub async fn write(&self, method: Method, path: &str, body: Value) -> Result<Value> {
    self.require()?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(300), Duration::from_millis(900))
      .await;
    self.call(method, path, Vec::new(), Some(body)).await
  }

  async fn call(
    &self,
    method: Method,
    path: &str,
    params: Vec<(&str, String)>,
    body: Option<Value>,
  ) -> Result<Value> {
    let mut req = self
      .ctx
      .http
      .request(method, format!("{BASE}{path}"))
      .no_cookies()
      .header("user-agent", USER_AGENT)
      .header("accept", "application/json")
      .queries(params);
    if let Some(token) = self.token() {
      req = req.header("authorization", format!("Bearer {token}"));
    }
    if let Some(body) = &body {
      req = req.json(body);
    }
    check(req.send().await?)
  }
}

fn check(resp: Resp) -> Result<Value> {
  let status = resp.status.as_u16();
  if (200..300).contains(&status) {
    if resp.body.iter().all(u8::is_ascii_whitespace) {
      return Ok(Value::Null);
    }
    return resp.value();
  }
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
  let message = v
    .str("description")
    .or_else(|| v.str("title"))
    .unwrap_or_default();
  let text = format!("Bangumi: {message}");
  Err(match status {
    _ if message.is_empty() => status_error(resp.status, &resp.text()),
    401 => Error::auth(text)
      .with_hint("the token was refused; create a new one: `atp guide login bangumi`"),
    403 => Error::new(ErrorCode::PermissionDenied, text),
    404 => Error::not_found(text),
    400 | 422 => Error::input(text),
    429 => Error::new(ErrorCode::RateLimited, text),
    _ => Error::upstream(text),
  })
}
