//! VoceChat transport (`https://<server>/api/...`), in one of two modes:
//!
//! - **user**: an access token in `X-API-Key`, from a password login; it
//!   expires within minutes and is renewed with the refresh token
//!   (`/token/renew`) when the server answers 401;
//! - **bot**: a bot's API key (`/bot/...`): send messages, list its
//!   channels, look up users and channels.
//!
//! Credentials come from `VOCECHAT_URL` + `VOCECHAT_API_KEY` (a bot key)
//! when set, else from the session (`login --token`).

use std::cell::RefCell;
use std::time::Duration;

use social_core::http::{Method, Part, Req};
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

const SERVER_KEY: &str = "server";
const TOKEN_KEY: &str = "token";
const REFRESH_KEY: &str = "refresh_token";
const BOT_KEY: &str = "bot_key";

#[derive(Clone, PartialEq, Eq)]
pub enum Mode {
  User,
  Bot,
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<(String, String)>,
  renewed: RefCell<bool>,
}

/// The request body of a message: its content type and bytes.
pub enum Content {
  Text(String),
  Markdown(String),
  File(String),
}

impl Content {
  fn parts(&self) -> (&'static str, Vec<u8>) {
    match self {
      Content::Text(t) => ("text/plain", t.as_bytes().to_vec()),
      Content::Markdown(t) => ("text/markdown", t.as_bytes().to_vec()),
      Content::File(path) => (
        "vocechat/file",
        json!({"path": path}).to_string().into_bytes(),
      ),
    }
  }
}

pub fn normalize(s: &str) -> String {
  let s = s.trim().trim_end_matches('/');
  if s.starts_with("https://") || s.starts_with("http://") {
    s.to_owned()
  } else {
    format!("https://{s}")
  }
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let env = match (var("VOCECHAT_URL"), var("VOCECHAT_API_KEY")) {
      (Some(url), Some(key)) => Some((normalize(&url), key)),
      _ => None,
    };
    Self {
      ctx,
      env,
      renewed: RefCell::new(false),
    }
  }

  pub fn server(&self) -> Option<String> {
    match &self.env {
      Some((url, _)) => Some(url.clone()),
      None => self.ctx.extra(SERVER_KEY),
    }
  }

  pub fn mode(&self) -> Option<Mode> {
    if self.env.is_some() || self.ctx.extra(BOT_KEY).is_some() {
      Some(Mode::Bot)
    } else if self.ctx.extra(TOKEN_KEY).is_some() {
      Some(Mode::User)
    } else {
      None
    }
  }

  pub fn logged_in(&self) -> bool {
    self.server().is_some() && self.mode().is_some()
  }

  fn key(&self) -> Option<String> {
    match &self.env {
      Some((_, k)) => Some(k.clone()),
      None => self
        .ctx
        .extra(BOT_KEY)
        .or_else(|| self.ctx.extra(TOKEN_KEY)),
    }
  }

  pub fn require(&self) -> Result<(String, Mode)> {
    match (self.server(), self.mode()) {
      (Some(s), Some(m)) => Ok((s, m)),
      _ => Err(Error::auth("not logged in to a VoceChat server").with_hint(
        "run `social vocechat login --token 'SERVER|BOT_API_KEY'` or `SERVER|EMAIL|PASSWORD` through `--token -`",
      )),
    }
  }

  /// Fail for what only a user (not a bot key) may do.
  pub fn require_user(&self, what: &str) -> Result<String> {
    let (server, mode) = self.require()?;
    if mode == Mode::Bot {
      return Err(Error::unsupported(what).with_hint(
        "a VoceChat bot key can only send messages and look up users and channels; log in as a user for this",
      ));
    }
    Ok(server)
  }

  pub fn save_user(&self, server: &str, token: &str, refresh: &str) {
    self.ctx.set_extra(SERVER_KEY, server);
    self.ctx.set_extra(TOKEN_KEY, token);
    self.ctx.set_extra(REFRESH_KEY, refresh);
  }

  pub fn save_bot(&self, server: &str, key: &str) {
    self.ctx.set_extra(SERVER_KEY, server);
    self.ctx.set_extra(BOT_KEY, key);
  }

  fn request(&self, method: Method, server: &str, path: &str, key: &str) -> Req<'_> {
    self
      .ctx
      .http
      .request(method, format!("{server}/api{path}"))
      .no_cookies()
      .retries(0)
      .header("x-api-key", key)
  }

  /// One call. `body`: JSON, raw content, or a multipart form.
  pub async fn call(&self, method: Method, path: &str, body: Body) -> Result<Value> {
    let (server, _) = self.require()?;
    loop {
      let key = self.key().unwrap_or_default();
      let mut req = self.request(method.clone(), &server, path, &key);
      req = match &body {
        Body::None => req,
        Body::Json(v) => req.json(v),
        Body::Content(c) => {
          let (ct, data) = c.parts();
          req.bytes(data, ct)
        }
        Body::Multipart(parts) => req
          .multipart(parts.clone())
          .timeout(Duration::from_secs(300)),
      };
      let resp = req.send().await?;
      let status = resp.status.as_u16();
      if status == 401 && self.mode() == Some(Mode::User) && !*self.renewed.borrow() {
        self.renew(&server).await?;
        continue;
      }
      if resp.status.is_success() {
        if resp.body.iter().all(u8::is_ascii_whitespace) {
          return Ok(Value::Null);
        }
        return Ok(serde_json::from_slice(&resp.body).unwrap_or_else(|_| json!(resp.text())));
      }
      let text = resp.text();
      let detail = if text.trim().is_empty() || text.contains("<html") {
        format!("HTTP {status}")
      } else {
        format!("{} (HTTP {status})", text.trim())
      };
      return Err(match status {
        401 => Error::auth(format!("VoceChat refused the credentials: {detail}"))
          .with_hint("log in again (`social vocechat login --token ...`)"),
        403 => Error::new(ErrorCode::PermissionDenied, detail),
        404 => Error::not_found(detail),
        429 => Error::new(ErrorCode::RateLimited, detail),
        400 | 409 | 413 | 422 => Error::input(detail),
        _ => Error::upstream(format!("VoceChat: {detail}")),
      });
    }
  }

  pub async fn get(&self, path: &str) -> Result<Value> {
    self.call(Method::GET, path, Body::None).await
  }

  /// Swap the expired access token for a new one.
  async fn renew(&self, server: &str) -> Result<()> {
    *self.renewed.borrow_mut() = true;
    let (Some(token), Some(refresh)) = (self.ctx.extra(TOKEN_KEY), self.ctx.extra(REFRESH_KEY))
    else {
      return Err(Error::auth("the VoceChat session expired; log in again"));
    };
    let resp = self
      .ctx
      .http
      .post(format!("{server}/api/token/renew"))
      .no_cookies()
      .json(&json!({"token": token, "refresh_token": refresh}))
      .send()
      .await?;
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    match (v.str("token"), v.str("refresh_token")) {
      (Some(t), Some(r)) if resp.status.is_success() => {
        self.save_user(server, &t, &r);
        Ok(())
      }
      _ => Err(
        Error::auth("the VoceChat session expired and could not be renewed")
          .with_hint("log in again (`social vocechat login --token 'SERVER|EMAIL|PASSWORD'`)"),
      ),
    }
  }

  /// The current access token (renewed first if needed), for the event stream.
  pub async fn fresh_token(&self) -> Result<String> {
    self.require_user("listen")?;
    // A cheap call that renews an expired token on the way.
    self.get("/user/me").await?;
    self
      .ctx
      .extra(TOKEN_KEY)
      .ok_or_else(|| Error::auth("not logged in"))
  }
}

#[derive(Clone)]
pub enum Body {
  None,
  Json(Value),
  Content(ContentBody),
  Multipart(Vec<Part>),
}

/// A cloneable message body.
#[derive(Clone)]
pub struct ContentBody(pub String, pub &'static str);

impl ContentBody {
  fn parts(&self) -> (&'static str, Vec<u8>) {
    (self.1, self.0.as_bytes().to_vec())
  }
}

impl From<Content> for Body {
  fn from(c: Content) -> Self {
    let (ct, data) = c.parts();
    Body::Content(ContentBody(String::from_utf8(data).unwrap_or_default(), ct))
  }
}
