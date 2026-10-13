//! Discord transport: the REST API (`discord.com/api/v9`) with a user token,
//! sent as the web client sends it, or a bot token (`Bot …`).
//!
//! The token comes from `DISCORD_BOT_TOKEN` / `DISCORD_TOKEN` when set, else
//! from the session (`login --token` or the QR login). User requests carry
//! the web client's headers (`X-Super-Properties` with the current build
//! number, locale, timezone); bot requests the `DiscordBot` user agent.
//! Rate limits are honored (`retry_after` of a 429, an exhausted bucket's
//! `X-RateLimit-Reset-After`) and Discord's JSON errors (`{code, message}`)
//! are mapped onto `atp_core::Error`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use atp_core::credentials;
use atp_core::http::{Method, Part, Req, USER_AGENT};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

use crate::parse;

mod client;
mod errors;

use self::errors::{retry_after, sleep_secs};

pub const WEB: &str = "https://discord.com";
pub const API: &str = "https://discord.com/api/v9";
const TOKEN_KEY: &str = "token";
const KIND_KEY: &str = "token_kind";
const MAX_TRIES: u32 = 4;
/// Longest wait for a rate limit before giving up.
const MAX_WAIT: f64 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  User,
  Bot,
}

impl Kind {
  pub fn name(self) -> &'static str {
    match self {
      Kind::User => "user",
      Kind::Bot => "bot",
    }
  }
}

#[derive(Clone)]
pub struct Token {
  pub secret: String,
  pub kind: Kind,
}

impl Token {
  /// `Bot xxx` / `bot:xxx` name a bot token; anything else is unknown.
  pub fn parse(raw: &str) -> (String, Option<Kind>) {
    let raw = raw.trim().trim_matches('"');
    for prefix in ["Bot ", "bot ", "Bot:", "bot:"] {
      if let Some(rest) = raw.strip_prefix(prefix) {
        return (rest.trim().to_owned(), Some(Kind::Bot));
      }
    }
    (raw.to_owned(), None)
  }

  /// The `Authorization` header value.
  pub fn header(&self) -> String {
    match self.kind {
      Kind::Bot => format!("Bot {}", self.secret),
      Kind::User => self.secret.clone(),
    }
  }
}

/// What a browser sends when the Discord web app opens a websocket.
pub const SOCKET_HEADERS: [(&str, &str); 5] = [
  ("Origin", WEB),
  ("User-Agent", USER_AGENT),
  ("Accept-Language", "en-US,en;q=0.9"),
  ("Cache-Control", "no-cache"),
  ("Pragma", "no-cache"),
];

/// What a request carries.
#[derive(Clone)]
pub enum Body {
  None,
  Json(Value),
  Multipart(Vec<Part>),
}

/// Where a channel lives and what it is called, for links and chat names.
#[derive(Debug, Clone)]
pub struct ChannelInfo {
  /// Guild id, or `@me` for direct and group chats.
  pub guild: String,
  pub name: String,
  pub kind: u64,
}

pub struct Api {
  pub ctx: Ctx,
  pub(crate) env: Option<Token>,
  props: RefCell<Option<String>>,
  channels: RefCell<HashMap<String, ChannelInfo>>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env = match (
      credentials::env("DISCORD_BOT_TOKEN"),
      credentials::env("DISCORD_TOKEN"),
    ) {
      (Some(bot), _) => Some(Token {
        secret: Token::parse(&bot).0,
        kind: Kind::Bot,
      }),
      (None, Some(raw)) => {
        let (secret, kind) = Token::parse(&raw);
        Some(Token {
          secret,
          kind: kind.unwrap_or(Kind::User),
        })
      }
      _ => None,
    };
    Self {
      ctx,
      env,
      props: RefCell::new(None),
      channels: RefCell::new(HashMap::new()),
    }
  }

  // ── credentials ──────────────────────────────────────────────────────

  pub fn token(&self) -> Option<Token> {
    if let Some(t) = &self.env {
      return Some(t.clone());
    }
    let secret = self.ctx.extra(TOKEN_KEY).filter(|t| !t.is_empty())?;
    let kind = match self.ctx.extra(KIND_KEY).as_deref() {
      Some("bot") => Kind::Bot,
      _ => Kind::User,
    };
    Some(Token { secret, kind })
  }

  pub fn logged_in(&self) -> bool {
    self.token().is_some()
  }

  pub fn is_bot(&self) -> bool {
    self.token().is_some_and(|t| t.kind == Kind::Bot)
  }

  pub fn require(&self) -> Result<Token> {
    self.token().ok_or_else(|| {
      Error::auth("not logged in to Discord")
        .with_hint("run `atp discord login` (QR code) or `atp discord login --token '...'`")
    })
  }

  /// Fail with `unsupported` for operations Discord only offers user accounts.
  pub fn require_user(&self, what: &str) -> Result<Token> {
    let token = self.require()?;
    if token.kind == Kind::Bot {
      return Err(
        Error::unsupported(what).with_hint("Discord offers this to user accounts, not to bots"),
      );
    }
    Ok(token)
  }

  pub fn save_token(&self, token: &Token) {
    self.ctx.set_extra(TOKEN_KEY, &token.secret);
    self.ctx.set_extra(KIND_KEY, token.kind.name());
  }

  // ── requests ─────────────────────────────────────────────────────────

  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let token = self.require()?;
    self
      .send(Some(&token), Method::GET, path, query, Body::None)
      .await
  }

  pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
    self.write(Method::POST, path, Body::Json(body)).await
  }

  pub async fn patch(&self, path: &str, body: Value) -> Result<Value> {
    self.write(Method::PATCH, path, Body::Json(body)).await
  }

  pub async fn put(&self, path: &str) -> Result<Value> {
    self.write(Method::PUT, path, Body::None).await
  }

  pub async fn delete(&self, path: &str) -> Result<Value> {
    self.write(Method::DELETE, path, Body::None).await
  }

  pub async fn post_multipart(&self, path: &str, parts: Vec<Part>) -> Result<Value> {
    self.write(Method::POST, path, Body::Multipart(parts)).await
  }

  /// A write: logged in, after a short human-like pause.
  async fn write(&self, method: Method, path: &str, body: Body) -> Result<Value> {
    let token = self.require()?;
    if token.kind == Kind::User {
      self
        .ctx
        .http
        .pause(Duration::from_millis(400), Duration::from_millis(1200))
        .await;
    }
    self.send(Some(&token), method, path, &[], body).await
  }

  /// One API call with `token` (or none), retrying rate limits.
  pub async fn send(
    &self,
    token: Option<&Token>,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Body,
  ) -> Result<Value> {
    let mut tries = 0;
    loop {
      tries += 1;
      let req = self
        .request(token, method.clone(), path)
        .await
        .queries(query.iter().map(|(k, v)| (*k, v)));
      let req = match &body {
        Body::None => req,
        Body::Json(v) => req.json(v),
        Body::Multipart(parts) => req.multipart(parts.clone()),
      };
      let resp = req.send().await?;
      let status = resp.status.as_u16();
      // Rate limited, or (search) an index Discord is still building.
      let wait = match status {
        429 => Some(retry_after(&resp).unwrap_or(2.0)),
        202 if method == Method::GET => retry_after(&resp),
        500..=599 if method == Method::GET => Some(f64::from(tries)),
        _ => None,
      };
      if let Some(wait) = wait {
        if tries < MAX_TRIES && wait <= MAX_WAIT {
          tracing::debug!("{path}: waiting {wait:.1}s ({status})");
          sleep_secs(wait).await;
          continue;
        }
        if status == 429 {
          return Err(Error::new(
            ErrorCode::RateLimited,
            format!("Discord rate limit reached (retry in {wait:.0}s)"),
          ));
        }
      }
      if resp.header("x-ratelimit-remaining") == Some("0")
        && let Some(reset) = resp
          .header("x-ratelimit-reset-after")
          .and_then(|s| s.parse::<f64>().ok())
      {
        // The bucket is empty: the next request on it would be refused.
        sleep_secs(reset.min(MAX_WAIT)).await;
      }
      return self.check(token, resp);
    }
  }

  async fn request(&self, token: Option<&Token>, method: Method, path: &str) -> Req<'_> {
    let url = if path.starts_with("https://") {
      path.to_owned()
    } else {
      format!("{API}{path}")
    };
    let mut req = self.ctx.http.request(method, url).no_cookies().retries(0);
    if let Some(t) = token {
      req = req.header("authorization", t.header());
    }
    if token.is_some_and(|t| t.kind == Kind::Bot) {
      return req.header(
        "user-agent",
        format!(
          "DiscordBot (https://github.com/WindustH/all-the-platforms, {})",
          env!("CARGO_PKG_VERSION")
        ),
      );
    }
    let tz = jiff::tz::TimeZone::system()
      .iana_name()
      .unwrap_or("UTC")
      .to_owned();
    req
      .header("x-super-properties", self.super_properties().await)
      .header("x-discord-locale", "en-US")
      .header("x-discord-timezone", tz)
      .header("x-debug-options", "bugReporterEnabled")
      .header("origin", WEB)
      .header("referer", format!("{WEB}/channels/@me"))
      .header("accept", "*/*")
  }

  // ── channels ─────────────────────────────────────────────────────────

  /// Guild and display name of a channel, looked up once per run.
  pub async fn channel(&self, id: &str) -> Result<ChannelInfo> {
    if let Some(info) = self.channels.borrow().get(id) {
      return Ok(info.clone());
    }
    let v = self.get(&format!("/channels/{id}"), &[]).await?;
    Ok(self.remember_channel(&v))
  }

  /// What is known about a channel without asking.
  pub fn cached(&self, id: &str) -> Option<ChannelInfo> {
    self.channels.borrow().get(id).cloned()
  }

  /// Keep what a channel payload says about where it lives.
  pub fn remember_channel(&self, v: &Value) -> ChannelInfo {
    let info = ChannelInfo {
      guild: v.str("guild_id").unwrap_or_else(|| "@me".into()),
      name: parse::chat_name(v),
      kind: v.u64("type").unwrap_or_default(),
    };
    if let Some(id) = v.str("id") {
      self.channels.borrow_mut().insert(id, info.clone());
    }
    info
  }
}
