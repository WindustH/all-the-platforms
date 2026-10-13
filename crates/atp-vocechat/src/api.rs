//! VoceChat transport (`https://<server>/api/...`), in one of two modes:
//!
//! - **user**: an access token in `X-API-Key`, from a password login; it
//!   expires within minutes and is renewed with the refresh token
//!   (`/token/renew`) when the server answers 401;
//! - **bot**: a bot's API key (`/bot/...`): send messages, list its
//!   channels, look up users and channels.
//!
//! Credentials come from `VOCECHAT_URL` + `VOCECHAT_API_KEY` (a bot key)
//! when set, else from the session ([`crate::login`]).

use std::cell::RefCell;
use std::time::Duration;

use atp_core::credentials;
use atp_core::http::{Method, Part, Req, Resp};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

use crate::parse::Users;

const SERVER_KEY: &str = "server";
const TOKEN_KEY: &str = "token";
const REFRESH_KEY: &str = "refresh_token";
const BOT_KEY: &str = "bot_key";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
  User,
  Bot,
}

/// What a request carries.
#[derive(Clone)]
pub enum Body {
  None,
  Json(Value),
  /// Message content with its VoceChat content type.
  Raw {
    data: String,
    content_type: &'static str,
  },
  Multipart(Vec<Part>),
}

/// The content of a message.
pub enum Content {
  Text(String),
  Markdown(String),
  /// A file uploaded before, by its server path.
  File(String),
}

impl From<Content> for Body {
  fn from(content: Content) -> Self {
    let (data, content_type) = match content {
      Content::Text(t) => (t, "text/plain"),
      Content::Markdown(t) => (t, "text/markdown"),
      Content::File(path) => (json!({"path": path}).to_string(), "vocechat/file"),
    };
    Body::Raw { data, content_type }
  }
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<(String, String)>,
  renewed: RefCell<bool>,
  /// The server's users, loaded once per run (user mode).
  pub users: RefCell<Option<Users>>,
  /// The account's user id, once asked.
  pub me: RefCell<Option<i64>>,
}

/// `chat.example.com` → `https://chat.example.com`, without a trailing `/`.
pub fn server_url(server: &str) -> String {
  let server = server.trim().trim_end_matches('/');
  if server.starts_with("https://") || server.starts_with("http://") {
    server.to_owned()
  } else {
    format!("https://{server}")
  }
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env = match (
      credentials::env("VOCECHAT_URL"),
      credentials::env("VOCECHAT_API_KEY"),
    ) {
      (Some(url), Some(key)) => Some((server_url(&url), key)),
      _ => None,
    };
    Self {
      ctx,
      env,
      renewed: RefCell::new(false),
      users: RefCell::new(None),
      me: RefCell::new(None),
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

  pub fn is_bot(&self) -> bool {
    self.mode() == Some(Mode::Bot)
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
        "run `atp vocechat login --token 'SERVER|BOT_API_KEY'` or `SERVER|EMAIL|PASSWORD` through `--token -`",
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
    credentials::save(
      &self.ctx,
      &[
        (SERVER_KEY, server),
        (TOKEN_KEY, token),
        (REFRESH_KEY, refresh),
      ],
    );
  }

  pub fn save_bot(&self, server: &str, key: &str) {
    credentials::save(&self.ctx, &[(SERVER_KEY, server), (BOT_KEY, key)]);
  }

  fn request(&self, method: Method, server: &str, path: &str, body: &Body) -> Req<'_> {
    let key = self.key().unwrap_or_default();
    let req = self
      .ctx
      .http
      .request(method, format!("{server}/api{path}"))
      .no_cookies()
      .retries(0)
      .header("x-api-key", key);
    match body {
      Body::None => req,
      Body::Json(v) => req.json(v),
      Body::Raw { data, content_type } => req.bytes(data.as_bytes().to_vec(), content_type),
      Body::Multipart(parts) => req
        .multipart(parts.clone())
        .timeout(Duration::from_secs(300)),
    }
  }

  /// One call; a user's expired token is renewed once on the way.
  pub async fn call(&self, method: Method, path: &str, body: Body) -> Result<Value> {
    let (server, _) = self.require()?;
    loop {
      let resp = self
        .request(method.clone(), &server, path, &body)
        .send()
        .await?;
      let expired =
        resp.status.as_u16() == 401 && self.mode() == Some(Mode::User) && !*self.renewed.borrow();
      if expired {
        self.renew(&server).await?;
        continue;
      }
      return check(resp);
    }
  }

  pub async fn get(&self, path: &str) -> Result<Value> {
    self.call(Method::GET, path, Body::None).await
  }

  /// A write, after the platform's short pause.
  pub async fn write(&self, method: Method, path: &str, body: Body) -> Result<Value> {
    self
      .ctx
      .http
      .pause(Duration::from_millis(200), Duration::from_millis(600))
      .await;
    self.call(method, path, body).await
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
          .with_hint("log in again (`atp vocechat login --token 'SERVER|EMAIL|PASSWORD'`)"),
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

/// The JSON of a success (`Null` when empty, a string when not JSON), or
/// the mapped error.
fn check(resp: Resp) -> Result<Value> {
  let status = resp.status.as_u16();
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
  Err(match status {
    401 => Error::auth(format!("VoceChat refused the credentials: {detail}"))
      .with_hint("log in again (`atp vocechat login --token ...`)"),
    403 => Error::new(ErrorCode::PermissionDenied, detail),
    404 => Error::not_found(detail),
    429 => Error::new(ErrorCode::RateLimited, detail),
    400 | 409 | 413 | 422 => Error::input(detail),
    _ => Error::upstream(format!("VoceChat: {detail}")),
  })
}
