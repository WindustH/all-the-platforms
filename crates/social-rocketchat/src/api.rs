//! Rocket.Chat transport: a server's REST API (`<server>/api/v1`) with the
//! `X-User-Id` / `X-Auth-Token` headers.
//!
//! Credentials come from `ROCKETCHAT_URL`, `ROCKETCHAT_USER_ID` and
//! `ROCKETCHAT_TOKEN` when set, else from the session: `login --token
//! 'SERVER USER_ID TOKEN'` (a personal access token) or `'SERVER USERNAME
//! PASSWORD'` (a password login, whose auth token is saved). Errors
//! (`{success: false, error}`) map onto `social_core::Error`; rate limits
//! (429) are waited out.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use social_core::http::{Method, Part};
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

use crate::rooms::RoomInfo;

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

pub fn server_url(s: &str) -> String {
  let s = s.trim().trim_end_matches('/');
  if s.starts_with("http://") || s.starts_with("https://") {
    s.to_owned()
  } else {
    format!("https://{s}")
  }
}

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
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let env = match (
      var("ROCKETCHAT_URL"),
      var("ROCKETCHAT_USER_ID"),
      var("ROCKETCHAT_TOKEN"),
    ) {
      (Some(server), Some(user_id), Some(token)) => Some(Creds {
        server: server_url(&server),
        user_id: user_id.trim().to_owned(),
        token: token.trim().to_owned(),
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
    let get = |k: &str| self.ctx.extra(k).filter(|v| !v.is_empty());
    Some(Creds {
      server: get(SERVER_KEY)?,
      user_id: get(USER_KEY)?,
      token: get(TOKEN_KEY)?,
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

  fn save(&self, creds: &Creds) {
    self.ctx.set_extra(SERVER_KEY, &creds.server);
    self.ctx.set_extra(USER_KEY, &creds.user_id);
    self.ctx.set_extra(TOKEN_KEY, &creds.token);
  }

  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let creds = self.require()?;
    self
      .send(Some(&creds), Method::GET, path, query, None)
      .await
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
    self
      .send(Some(&creds), Method::POST, path, &[], Some(body))
      .await
  }

  pub async fn send(
    &self,
    creds: Option<&Creds>,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<Body>,
  ) -> Result<Value> {
    let server = match creds {
      Some(c) => c.server.clone(),
      None => return Err(Error::internal("no server")),
    };
    let mut tries = 0;
    loop {
      tries += 1;
      let mut req = self
        .ctx
        .http
        .request(method.clone(), format!("{server}/api/v1/{path}"))
        .no_cookies()
        .retries(0)
        .queries(query.iter().map(|(k, v)| (*k, v)));
      if let Some(c) = creds {
        for (k, v) in c.headers().into_iter().filter(|(_, v)| !v.is_empty()) {
          req = req.header(&k, v);
        }
      }
      req = match &body {
        None => req,
        Some(Body::Json(v)) => req.json(v),
        Some(Body::Multipart(parts)) => req.multipart(parts.clone()),
      };
      let resp = req.send().await?;
      let status = resp.status.as_u16();
      if status == 429 && tries < MAX_TRIES {
        // X-RateLimit-Reset is the reset time in milliseconds.
        let now = jiff::Timestamp::now().as_millisecond();
        let wait = resp
          .header("x-ratelimit-reset")
          .and_then(|s| s.parse::<i64>().ok())
          .map(|reset| ((reset - now) / 1000).clamp(1, 60) as u64)
          .unwrap_or(5);
        tokio::time::sleep(Duration::from_secs(wait)).await;
        continue;
      }
      if resp.status.is_server_error() && method == Method::GET && tries < MAX_TRIES {
        tokio::time::sleep(Duration::from_secs(u64::from(tries))).await;
        continue;
      }
      let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
      if resp.status.is_success() && v.bool("success") != Some(false) {
        return Ok(v);
      }
      let message = v
        .first_str(&["error", "message"])
        .unwrap_or_else(|| format!("HTTP {status}"));
      let text = format!("Rocket.Chat: {message}");
      return Err(match status {
        401 => Error::auth(text).with_hint(if self.env.is_some() {
          "check ROCKETCHAT_USER_ID and ROCKETCHAT_TOKEN"
        } else {
          "the token was revoked or expired; log in again"
        }),
        403 => Error::new(ErrorCode::PermissionDenied, text),
        404 => Error::not_found(text),
        429 => Error::new(ErrorCode::RateLimited, text),
        _ if message.contains("not-found")
          || message.contains("not found")
          || message.contains("invalid-room")
          || message.contains("invalid-channel") =>
        {
          Error::not_found(text)
        }
        400 => Error::input(text),
        _ => Error::upstream(text),
      });
    }
  }
}

/// Log in: `SERVER USER_ID TOKEN` or `SERVER USERNAME PASSWORD` (split on
/// `|` when present, else on whitespace).
pub async fn login(api: &Api, raw: &str) -> Result<()> {
  let parts: Vec<&str> = if raw.contains('|') {
    raw.split('|').map(str::trim).collect()
  } else {
    raw.split_whitespace().collect()
  };
  let [server, first, second] = parts.as_slice() else {
    return Err(Error::input(
      "expected 'SERVER USER_ID TOKEN' or 'SERVER USERNAME PASSWORD'",
    ));
  };
  let server = server_url(server);
  let as_token = Creds {
    server: server.clone(),
    user_id: (*first).to_owned(),
    token: (*second).to_owned(),
  };
  let creds = match api
    .send(Some(&as_token), Method::GET, "me", &[], None)
    .await
  {
    Ok(_) => as_token,
    Err(e) if e.code == ErrorCode::NotAuthenticated => {
      let anon = Creds {
        server: server.clone(),
        user_id: String::new(),
        token: String::new(),
      };
      let v = api
        .send(
          Some(&anon),
          Method::POST,
          "login",
          &[],
          Some(Body::Json(json!({"user": first, "password": second}))),
        )
        .await
        .map_err(|e| {
          e.with_hint("neither a user id + token nor a username + password; accounts with 2FA need a personal access token")
        })?;
      Creds {
        server,
        user_id: v
          .str("data.userId")
          .ok_or_else(|| Error::upstream("the login answer carried no user id"))?,
        token: v
          .str("data.authToken")
          .ok_or_else(|| Error::upstream("the login answer carried no token"))?,
      }
    }
    Err(e) => return Err(e),
  };
  api.save(&creds);
  Ok(())
}
