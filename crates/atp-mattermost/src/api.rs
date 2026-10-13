//! Mattermost transport: a server's REST API (`<server>/api/v4`) with a
//! personal access token or a session token as bearer.
//!
//! The server and token come from `MATTERMOST_URL` / `MATTERMOST_TOKEN`
//! when set, else from the session ([`crate::login`]). Errors (`{id,
//! message, status_code}`) map onto `atp_core::Error`; 429 and server
//! errors are retried.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use atp_core::credentials;
use atp_core::http::{Method, Part, Req, Resp};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

use crate::chats::ChannelInfo;
use crate::users::Brief;

const SERVER_KEY: &str = "server";
const TOKEN_KEY: &str = "token";
const USER_KEY: &str = "user_id";
const MAX_TRIES: u32 = 4;

#[derive(Clone)]
pub struct Creds {
  pub server: String,
  pub token: String,
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
  /// Users seen so far, for authors and member lists.
  pub users: RefCell<HashMap<String, Brief>>,
  /// Channels seen so far: names, kinds, teams.
  pub channels: RefCell<HashMap<String, ChannelInfo>>,
  /// Team names by id, for links.
  pub teams: RefCell<HashMap<String, String>>,
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

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env = match (
      credentials::env("MATTERMOST_URL"),
      credentials::env("MATTERMOST_TOKEN"),
    ) {
      (Some(server), Some(token)) => Some(Creds {
        server: server_url(&server),
        token,
      }),
      _ => None,
    };
    Self {
      ctx,
      env,
      users: RefCell::new(HashMap::new()),
      channels: RefCell::new(HashMap::new()),
      teams: RefCell::new(HashMap::new()),
    }
  }

  pub fn creds(&self) -> Option<Creds> {
    if let Some(c) = &self.env {
      return Some(c.clone());
    }
    let saved = credentials::saved(&self.ctx, &[SERVER_KEY, TOKEN_KEY])?;
    let [server, token] = <[String; 2]>::try_from(saved).ok()?;
    Some(Creds { server, token })
  }

  pub fn logged_in(&self) -> bool {
    self.creds().is_some()
  }

  pub fn require(&self) -> Result<Creds> {
    self.creds().ok_or_else(|| {
      Error::auth("not logged in to Mattermost").with_hint(
        "run `atp mattermost login --token 'https://SERVER TOKEN'` or set MATTERMOST_URL and MATTERMOST_TOKEN",
      )
    })
  }

  pub fn server(&self) -> Result<String> {
    Ok(self.require()?.server)
  }

  pub fn save(&self, creds: &Creds, user_id: &str) {
    credentials::save(
      &self.ctx,
      &[
        (SERVER_KEY, creds.server.as_str()),
        (TOKEN_KEY, creds.token.as_str()),
        (USER_KEY, user_id),
      ],
    );
  }

  /// The account's user id.
  pub async fn me(&self) -> Result<String> {
    if self.env.is_none()
      && let Some(id) = self.ctx.extra(USER_KEY).filter(|u| !u.is_empty())
    {
      return Ok(id);
    }
    let v = self.get("/users/me", &[]).await?;
    let id = v
      .str("id")
      .ok_or_else(|| Error::upstream("no user id for the account"))?;
    if self.env.is_none() {
      self.ctx.set_extra(USER_KEY, &id);
    }
    Ok(id)
  }

  // ── calls ────────────────────────────────────────────────────────────

  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let creds = self.require()?;
    self.send(&creds, Method::GET, path, query, None).await
  }

  /// A POST that only reads (search, lookups by id): no pause.
  pub async fn post_read(&self, path: &str, body: Value) -> Result<Value> {
    let creds = self.require()?;
    self
      .send(&creds, Method::POST, path, &[], Some(Body::Json(body)))
      .await
  }

  pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
    self.write(Method::POST, path, Some(Body::Json(body))).await
  }

  pub async fn put(&self, path: &str, body: Value) -> Result<Value> {
    self.write(Method::PUT, path, Some(Body::Json(body))).await
  }

  pub async fn delete(&self, path: &str) -> Result<Value> {
    self.write(Method::DELETE, path, None).await
  }

  pub async fn upload(&self, path: &str, parts: Vec<Part>) -> Result<Value> {
    self
      .write(Method::POST, path, Some(Body::Multipart(parts)))
      .await
  }

  async fn write(&self, method: Method, path: &str, body: Option<Body>) -> Result<Value> {
    let creds = self.require()?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(200), Duration::from_millis(600))
      .await;
    self.send(&creds, method, path, &[], body).await
  }

  /// One call, retried on 429 (and server errors of reads) as the
  /// `X-RateLimit-Reset` header says.
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
    let req = self
      .ctx
      .http
      .request(method.clone(), format!("{}/api/v4{path}", creds.server))
      .no_cookies()
      .retries(0)
      .header("authorization", format!("Bearer {}", creds.token))
      .header("x-requested-with", "XMLHttpRequest")
      .queries(query.iter().map(|(k, v)| (*k, v)));
    match body {
      None => req,
      Some(Body::Json(v)) => req.json(v),
      Some(Body::Multipart(parts)) => req.multipart(parts.clone()),
    }
  }

  /// The JSON of a success (`Null` for an empty body), or the mapped error.
  fn check(&self, resp: Resp) -> Result<Value> {
    if resp.status.is_success() {
      if resp.body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Null);
      }
      return resp.value();
    }
    let status = resp.status.as_u16();
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    let message = v.str("message").unwrap_or_else(|| format!("HTTP {status}"));
    let text = format!("Mattermost: {message}");
    Err(match status {
      401 => Error::auth(text).with_hint(if self.env.is_some() {
        "check MATTERMOST_TOKEN"
      } else {
        "the token was revoked or the session expired; log in again"
      }),
      403 => Error::new(ErrorCode::PermissionDenied, text),
      404 => Error::not_found(text),
      400 | 413 => Error::input(text),
      429 => Error::new(ErrorCode::RateLimited, text),
      501 => Error::new(ErrorCode::UnsupportedOperation, text)
        .with_hint("the server is older or has the feature turned off"),
      _ => Error::upstream(text),
    })
  }
}

/// How long to wait before retry number `tries`: the reset the server
/// names, else one second per try, at most 30.
fn retry_wait(resp: &Resp, tries: u32) -> Duration {
  let secs = resp
    .header("x-ratelimit-reset")
    .and_then(|s| s.parse::<u64>().ok())
    .unwrap_or(u64::from(tries))
    .min(30);
  Duration::from_secs(secs.max(1))
}
