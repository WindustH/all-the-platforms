//! Discord transport: the REST API (`discord.com/api/v9`) with a user token,
//! sent as the web client sends it, or a bot token (`Bot …`).
//!
//! The token comes from `DISCORD_BOT_TOKEN` / `DISCORD_TOKEN` when set, else
//! from the session (`login --token` or the QR login). User requests carry
//! the web client's headers (`X-Super-Properties` with the current build
//! number, locale, timezone); bot requests the `DiscordBot` user agent.
//! Rate limits are honored (`retry_after` of a 429, an exhausted bucket's
//! `X-RateLimit-Reset-After`) and Discord's JSON errors (`{code, message}`)
//! are mapped onto `social_core::Error`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use social_core::http::{Method, Part, Req, Resp, USER_AGENT};
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

use crate::parse;

pub const WEB: &str = "https://discord.com";
pub const API: &str = "https://discord.com/api/v9";
const TOKEN_KEY: &str = "token";
const KIND_KEY: &str = "token_kind";
/// Client build of the web app when discord.com cannot be asked.
const BUILD_FALLBACK: u64 = 634_304;
const BUILD_TTL: Duration = Duration::from_secs(24 * 3600);
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
  env: Option<Token>,
  props: RefCell<Option<String>>,
  channels: RefCell<HashMap<String, ChannelInfo>>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let env = match (var("DISCORD_BOT_TOKEN"), var("DISCORD_TOKEN")) {
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
        .with_hint("run `social discord login` (QR code) or `social discord login --token '...'`")
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
          "DiscordBot (https://github.com/WindustH/social-cli, {})",
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

  /// `X-Super-Properties`: the web client's self-description, base64 JSON.
  pub async fn super_properties(&self) -> String {
    if let Some(p) = self.props.borrow().clone() {
      return p;
    }
    let props = STANDARD.encode(self.client_properties().await.to_string());
    *self.props.borrow_mut() = Some(props.clone());
    props
  }

  /// The client description sent in `X-Super-Properties` and the gateway identify.
  pub async fn client_properties(&self) -> Value {
    json!({
      "os": "Mac OS X",
      "browser": "Chrome",
      "device": "",
      "system_locale": "en-US",
      "has_client_mods": false,
      "browser_user_agent": USER_AGENT,
      "browser_version": "149.0.0.0",
      "os_version": "10.15.7",
      "referrer": "",
      "referring_domain": "",
      "referrer_current": "",
      "referring_domain_current": "",
      "release_channel": "stable",
      "client_build_number": self.build_number().await,
      "client_event_source": null,
    })
  }

  /// The web app's current build number, read from discord.com once a day.
  async fn build_number(&self) -> u64 {
    if let Some(n) = self.ctx.store.cache_get::<u64>("build", BUILD_TTL) {
      return n;
    }
    let page = self
      .ctx
      .http
      .get(format!("{WEB}/login"))
      .no_cookies()
      .header("accept", "text/html")
      .send()
      .await;
    let found = page.ok().and_then(|r| {
      let text = r.text();
      let at = text.find("\"BUILD_NUMBER\":\"")? + 16;
      text[at..].split('"').next()?.parse::<u64>().ok()
    });
    match found {
      Some(n) => {
        self.ctx.store.cache_put("build", &n);
        n
      }
      None => {
        tracing::debug!("no build number on discord.com; using {BUILD_FALLBACK}");
        BUILD_FALLBACK
      }
    }
  }

  /// Response → JSON (`Null` for empty bodies), or a shared error.
  fn check(&self, token: Option<&Token>, resp: Resp) -> Result<Value> {
    let status = resp.status.as_u16();
    if resp.status.is_success() {
      if resp.body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Null);
      }
      return resp.value();
    }
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    let message = v.str("message").unwrap_or_else(|| format!("HTTP {status}"));
    let code = v.i64("code").unwrap_or_default();
    if status == 401 || code == 40001 {
      let hint = if self.env.is_some() {
        "check DISCORD_TOKEN / DISCORD_BOT_TOKEN".to_owned()
      } else if token.is_some() {
        "the token may have been reset (password change, logout); log in again".to_owned()
      } else {
        "run `social discord login`".to_owned()
      };
      return Err(
        Error::auth(format!("Discord did not accept the token ({message})")).with_hint(hint),
      );
    }
    if v.at("captcha_key") != &Value::Null {
      return Err(
        Error::new(
          ErrorCode::VerificationRequired,
          "Discord asks for a captcha for this action",
        )
        .with_hint(
          "do the same once in the Discord app or website, then retry; bots never get captchas",
        ),
      );
    }
    let text = if code > 0 {
      format!("{message} (code {code})")
    } else {
      message
    };
    Err(match code {
      10003 | 10004 | 10006 | 10007 | 10008 | 10013 | 10014 | 10015 => Error::not_found(text),
      40002 => Error::new(ErrorCode::VerificationRequired, text)
        .with_hint("verify the account's email or phone in the Discord app"),
      20001 | 20002 => Error::new(ErrorCode::UnsupportedOperation, text),
      20016 | 20028 | 31001 => Error::new(ErrorCode::RateLimited, text),
      50035 => Error::input(match first_form_error(&v) {
        Some(detail) => format!("{text}: {detail}"),
        None => text,
      }),
      50001 | 50005 | 50007 | 50013 | 50021 | 50024 | 160002 => {
        Error::new(ErrorCode::PermissionDenied, text)
      }
      _ => match status {
        403 => Error::new(ErrorCode::PermissionDenied, text),
        404 => Error::not_found(text),
        413 => Error::input(format!("{text}: the file is too large for this channel")),
        429 => Error::new(ErrorCode::RateLimited, text),
        400 => Error::input(text),
        _ => Error::upstream(format!("Discord: {text} (HTTP {status})")),
      },
    })
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

fn retry_after(resp: &Resp) -> Option<f64> {
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
  v.f64("retry_after")
    .or_else(|| resp.header("retry-after").and_then(|s| s.parse().ok()))
}

async fn sleep_secs(secs: f64) {
  let jitter: f64 = rand::random_range(0.2..0.8);
  tokio::time::sleep(Duration::from_secs_f64(secs.max(0.0) + jitter)).await;
}

/// The first message inside `errors` of an "Invalid Form Body" answer.
fn first_form_error(v: &Value) -> Option<String> {
  fn walk(v: &Value, path: &str) -> Option<String> {
    if let Some(e) = v.list("_errors").first() {
      let message = e.str("message")?;
      return Some(if path.is_empty() {
        message
      } else {
        format!("{path}: {message}")
      });
    }
    v.as_object()?.iter().find_map(|(k, child)| {
      let path = if path.is_empty() {
        k.clone()
      } else {
        format!("{path}.{k}")
      };
      walk(child, &path)
    })
  }
  walk(v.at("errors"), "")
}
