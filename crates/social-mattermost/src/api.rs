//! Mattermost transport: a server's REST API (`<server>/api/v4`) with a
//! personal access token or a session token as bearer.
//!
//! The server and token come from `MATTERMOST_URL` / `MATTERMOST_TOKEN`
//! when set, else from the session (`login --token 'SERVER TOKEN'`, or
//! `'SERVER LOGIN PASSWORD'` for a password login that yields a session
//! token). Errors (`{id, message, status_code}`) map onto
//! `social_core::Error`; 429 and server errors are retried.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use social_core::http::{Method, Part};
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

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
pub fn server_url(s: &str) -> String {
  let s = s.trim().trim_end_matches('/');
  if s.starts_with("http://") || s.starts_with("https://") {
    s.to_owned()
  } else {
    format!("https://{s}")
  }
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let env = match (var("MATTERMOST_URL"), var("MATTERMOST_TOKEN")) {
      (Some(server), Some(token)) => Some(Creds {
        server: server_url(&server),
        token: token.trim().to_owned(),
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
    Some(Creds {
      server: self.ctx.extra(SERVER_KEY).filter(|s| !s.is_empty())?,
      token: self.ctx.extra(TOKEN_KEY).filter(|t| !t.is_empty())?,
    })
  }

  pub fn logged_in(&self) -> bool {
    self.creds().is_some()
  }

  pub fn require(&self) -> Result<Creds> {
    self.creds().ok_or_else(|| {
      Error::auth("not logged in to Mattermost").with_hint(
        "run `social mattermost login --token 'https://SERVER TOKEN'` or set MATTERMOST_URL and MATTERMOST_TOKEN",
      )
    })
  }

  pub fn server(&self) -> Result<String> {
    Ok(self.require()?.server)
  }

  pub fn save(&self, creds: &Creds, user_id: &str) {
    self.ctx.set_extra(SERVER_KEY, &creds.server);
    self.ctx.set_extra(TOKEN_KEY, &creds.token);
    self.ctx.set_extra(USER_KEY, user_id);
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
      let mut req = self
        .ctx
        .http
        .request(method.clone(), format!("{}/api/v4{path}", creds.server))
        .no_cookies()
        .retries(0)
        .header("authorization", format!("Bearer {}", creds.token))
        .header("x-requested-with", "XMLHttpRequest")
        .queries(query.iter().map(|(k, v)| (*k, v)));
      req = match &body {
        None => req,
        Some(Body::Json(v)) => req.json(v),
        Some(Body::Multipart(parts)) => req.multipart(parts.clone()),
      };
      let resp = req.send().await?;
      let status = resp.status.as_u16();
      let retry = status == 429 || (resp.status.is_server_error() && method == Method::GET);
      if retry && tries < MAX_TRIES {
        let wait = resp
          .header("x-ratelimit-reset")
          .and_then(|s| s.parse::<u64>().ok())
          .unwrap_or(u64::from(tries))
          .min(30);
        tokio::time::sleep(Duration::from_secs(wait.max(1))).await;
        continue;
      }
      if resp.status.is_success() {
        if resp.body.iter().all(u8::is_ascii_whitespace) {
          return Ok(Value::Null);
        }
        return resp.value();
      }
      let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
      let message = v.str("message").unwrap_or_else(|| format!("HTTP {status}"));
      let text = format!("Mattermost: {message}");
      return Err(match status {
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
      });
    }
  }
}

#[derive(Clone)]
pub enum Body {
  Json(Value),
  Multipart(Vec<Part>),
}

/// Log in: `SERVER TOKEN`, or `SERVER LOGIN PASSWORD` (split on `|` when
/// present, else on whitespace).
pub async fn login(api: &Api, raw: &str) -> Result<String> {
  let parts: Vec<&str> = if raw.contains('|') {
    raw.split('|').map(str::trim).collect()
  } else {
    raw.split_whitespace().collect()
  };
  let (server, token) = match parts.as_slice() {
    [server, token] => (server_url(server), (*token).to_owned()),
    [server, login, password] => {
      let server = server_url(server);
      let resp = api
        .ctx
        .http
        .post(format!("{server}/api/v4/users/login"))
        .no_cookies()
        .json(&json!({"login_id": login, "password": password}))
        .send()
        .await?;
      if !resp.status.is_success() {
        let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
        return Err(
          Error::auth(format!(
            "Mattermost refused the login: {}",
            v.str("message").unwrap_or_else(|| resp.status.to_string())
          ))
          .with_hint("accounts with MFA or SSO need a personal access token instead"),
        );
      }
      let token = resp
        .header("token")
        .ok_or_else(|| Error::upstream("the login answer carried no session token"))?
        .to_owned();
      (server, token)
    }
    _ => {
      return Err(Error::input(
        "expected 'SERVER TOKEN' or 'SERVER LOGIN PASSWORD'",
      ));
    }
  };
  let creds = Creds { server, token };
  let me = api
    .send(&creds, Method::GET, "/users/me", &[], None)
    .await?;
  let id = me
    .str("id")
    .ok_or_else(|| Error::upstream("no user id for the account"))?;
  api.save(&creds, &id);
  Ok(id)
}
