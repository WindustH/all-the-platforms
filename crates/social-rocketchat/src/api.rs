//! Rocket.Chat transport: a server's REST API (`<server>/api/v1`) with the
//! `X-User-Id` / `X-Auth-Token` headers.
//!
//! Credentials come from `ROCKETCHAT_URL`, `ROCKETCHAT_USER_ID` and
//! `ROCKETCHAT_TOKEN` when set, else from the session ([`crate::login`]).
//! Errors (`{success: false, error}`) map onto `social_core::Error`; rate
//! limits (429) are waited out.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use social_core::credentials;
use social_core::http::{Method, Part, Req, Resp};
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

use crate::chats::RoomInfo;

const SERVER_KEY: &str = "server";
const USER_KEY: &str = "user_id";
const TOKEN_KEY: &str = "token";
const MAX_TRIES: u32 = 4;

#[derive(Clone)]
pub struct Creds {
  pub server: String,
  pub user_id: String,
  pub token: String,
}

impl Creds {
  /// The headers every request (and file download) carries.
  pub fn headers(&self) -> Vec<(String, String)> {
    vec![
      ("x-user-id".into(), self.user_id.clone()),
      ("x-auth-token".into(), self.token.clone()),
    ]
  }
}

/// `chat.example.com` → `https://chat.example.com`, without a trailing `/`.
pub fn server_url(server: &str) -> String {
  let server = server.trim().trim_end_matches('/');
  if server.starts_with("http://") || server.starts_with("https://") {
    server.to_owned()
  } else {
    format!("https://{server}")
  }
}

/// What a request carries.
#[derive(Clone)]
pub enum Body {
  Json(Value),
  Multipart(Vec<Part>),
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<Creds>,
  /// Rooms seen so far: names, kinds, link paths.
  pub rooms: RefCell<HashMap<String, RoomInfo>>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env = match (
      credentials::env("ROCKETCHAT_URL"),
      credentials::env("ROCKETCHAT_USER_ID"),
      credentials::env("ROCKETCHAT_TOKEN"),
    ) {
      (Some(server), Some(user_id), Some(token)) => Some(Creds {
        server: server_url(&server),
        user_id,
        token,
      }),
      _ => None,
    };
    Self {
      ctx,
      env,
      rooms: RefCell::new(HashMap::new()),
    }
  }

  pub fn creds(&self) -> Option<Creds> {
    if let Some(c) = &self.env {
      return Some(c.clone());
    }
    let saved = credentials::saved(&self.ctx, &[SERVER_KEY, USER_KEY, TOKEN_KEY])?;
    let [server, user_id, token] = <[String; 3]>::try_from(saved).ok()?;
    Some(Creds {
      server,
      user_id,
      token,
    })
  }

  pub fn logged_in(&self) -> bool {
    self.creds().is_some()
  }

  pub fn require(&self) -> Result<Creds> {
    self.creds().ok_or_else(|| {
      Error::auth("not logged in to Rocket.Chat").with_hint(
        "run `social rocketchat login --token 'https://SERVER USER_ID TOKEN'` or set ROCKETCHAT_URL, ROCKETCHAT_USER_ID and ROCKETCHAT_TOKEN",
      )
    })
  }

  pub fn save(&self, creds: &Creds) {
    credentials::save(
      &self.ctx,
      &[
        (SERVER_KEY, creds.server.as_str()),
        (USER_KEY, creds.user_id.as_str()),
        (TOKEN_KEY, creds.token.as_str()),
      ],
    );
  }

  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let creds = self.require()?;
    self.send(&creds, Method::GET, path, query, None).await
  }

  pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
    self.write(path, Body::Json(body)).await
  }

  pub async fn upload(&self, path: &str, parts: Vec<Part>) -> Result<Value> {
    self.write(path, Body::Multipart(parts)).await
  }

  async fn write(&self, path: &str, body: Body) -> Result<Value> {
    let creds = self.require()?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(200), Duration::from_millis(600))
      .await;
    self.send(&creds, Method::POST, path, &[], Some(body)).await
  }

  /// One call, waiting out rate limits and retrying server errors of reads.
  pub async fn send(
    &self,
    creds: &Creds,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<Body>,
  ) -> Result<Value> {
    let mut tries = 0;
    loop {
      tries += 1;
      let req = self.request(creds, &method, path, query, body.as_ref());
      let resp = req.send().await?;
      let retry =
        resp.status.as_u16() == 429 || (resp.status.is_server_error() && method == Method::GET);
      if retry && tries < MAX_TRIES {
        tokio::time::sleep(retry_wait(&resp, tries)).await;
        continue;
      }
      return self.check(resp);
    }
  }

  fn request(
    &self,
    creds: &Creds,
    method: &Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Body>,
  ) -> Req<'_> {
    let mut req = self
      .ctx
      .http
      .request(method.clone(), format!("{}/api/v1/{path}", creds.server))
      .no_cookies()
      .retries(0)
      .queries(query.iter().map(|(k, v)| (*k, v)));
    for (k, v) in creds.headers().into_iter().filter(|(_, v)| !v.is_empty()) {
      req = req.header(&k, v);
    }
    match body {
      None => req,
      Some(Body::Json(v)) => req.json(v),
      Some(Body::Multipart(parts)) => req.multipart(parts.clone()),
    }
  }

  /// The JSON of a success, or the mapped error.
  fn check(&self, resp: Resp) -> Result<Value> {
    let status = resp.status.as_u16();
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    if resp.status.is_success() && v.bool("success") != Some(false) {
      return Ok(v);
    }
    let message = v
      .first_str(&["error", "message"])
      .unwrap_or_else(|| format!("HTTP {status}"));
    let text = format!("Rocket.Chat: {message}");
    let missing = ["not-found", "not found", "invalid-room", "invalid-channel"]
      .iter()
      .any(|m| message.contains(m));
    Err(match status {
      401 => Error::auth(text).with_hint(if self.env.is_some() {
        "check ROCKETCHAT_USER_ID and ROCKETCHAT_TOKEN"
      } else {
        "the token was revoked or expired; log in again"
      }),
      403 => Error::new(ErrorCode::PermissionDenied, text),
      404 => Error::not_found(text),
      429 => Error::new(ErrorCode::RateLimited, text),
      _ if missing => Error::not_found(text),
      400 => Error::input(text),
      _ => Error::upstream(text),
    })
  }
}

/// How long to wait before retry number `tries`: until the rate limit's
/// reset (`X-RateLimit-Reset`, in milliseconds) for a 429, else a second per
/// try.
fn retry_wait(resp: &Resp, tries: u32) -> Duration {
  if resp.status.as_u16() != 429 {
    return Duration::from_secs(u64::from(tries));
  }
  let now = jiff::Timestamp::now().as_millisecond();
  let secs = resp
    .header("x-ratelimit-reset")
    .and_then(|s| s.parse::<i64>().ok())
    .map(|reset| ((reset - now) / 1000).clamp(1, 60) as u64)
    .unwrap_or(5);
  Duration::from_secs(secs)
}
