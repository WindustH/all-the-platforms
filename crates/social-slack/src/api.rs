//! Slack transport: the Web API (`https://slack.com/api/<method>`) with a
//! bot (`xoxb-`) or user (`xoxp-`) token as bearer.
//!
//! Every method is called as a form POST. Slack answers `{ok: false, error}`
//! with HTTP 200; those errors (missing scopes, channels the app is not in,
//! revoked tokens ...) and 429 back-offs (`Retry-After`) are mapped onto
//! `social_core::Error`. Tokens come from `SLACK_TOKEN` (or
//! `SLACK_BOT_TOKEN` / `SLACK_USER_TOKEN`) when set, else from the session
//! (`login --token`); the app-level `xapp-` token for Socket Mode from
//! `SLACK_APP_TOKEN` or the session.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

use crate::chats::ChatInfo;
use crate::users::Brief;

pub const API: &str = "https://slack.com/api";
const TOKEN_KEY: &str = "token";
const APP_KEY: &str = "app_token";
const TEAM_URL_KEY: &str = "team_url";
const TEAM_KEY: &str = "team";
const USER_KEY: &str = "user_id";
const MAX_TRIES: u32 = 4;
const MAX_WAIT: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  Bot,
  User,
}

impl Kind {
  pub fn of(token: &str) -> Kind {
    if token.starts_with("xoxb-") {
      Kind::Bot
    } else {
      Kind::User
    }
  }

  pub fn name(self) -> &'static str {
    match self {
      Kind::Bot => "bot",
      Kind::User => "user",
    }
  }
}

/// What `auth.test` says about a token.
pub struct Identity {
  pub user_id: String,
  pub team: String,
  pub team_url: String,
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<String>,
  env_app: Option<String>,
  /// Users seen so far: names for authors and `<@U…>` mentions.
  pub users: RefCell<HashMap<String, Brief>>,
  /// Conversations seen so far: names and kinds.
  pub chats: RefCell<HashMap<String, ChatInfo>>,
  /// `auth.test` of this run: (user id, workspace URL).
  ident: RefCell<Option<(String, String)>>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let env = var("SLACK_TOKEN")
      .or_else(|| var("SLACK_USER_TOKEN"))
      .or_else(|| var("SLACK_BOT_TOKEN"));
    Self {
      ctx,
      env,
      env_app: var("SLACK_APP_TOKEN"),
      users: RefCell::new(HashMap::new()),
      chats: RefCell::new(HashMap::new()),
      ident: RefCell::new(None),
    }
  }

  // ── credentials ──────────────────────────────────────────────────────

  pub fn token(&self) -> Option<String> {
    self
      .env
      .clone()
      .or_else(|| self.ctx.extra(TOKEN_KEY).filter(|t| !t.is_empty()))
  }

  pub fn app_token(&self) -> Option<String> {
    self
      .env_app
      .clone()
      .or_else(|| self.ctx.extra(APP_KEY).filter(|t| !t.is_empty()))
  }

  pub fn logged_in(&self) -> bool {
    self.token().is_some()
  }

  pub fn kind(&self) -> Option<Kind> {
    self.token().map(|t| Kind::of(&t))
  }

  pub fn require(&self) -> Result<String> {
    self.token().ok_or_else(|| {
      Error::auth("not logged in to Slack").with_hint(
        "run `social slack login --token xoxb-…` (bot) or `--token xoxp-…` (user), or set SLACK_TOKEN",
      )
    })
  }

  /// Fail with `unsupported` for methods Slack only offers user tokens.
  pub fn require_user(&self, what: &str) -> Result<()> {
    self.require()?;
    if self.kind() == Some(Kind::Bot) {
      return Err(
        Error::unsupported(what)
          .with_hint("Slack offers this to user tokens (xoxp-…), not to bot tokens"),
      );
    }
    Ok(())
  }

  pub fn save(&self, token: &str, app: Option<&str>, id: &Identity) {
    self.ctx.set_extra(TOKEN_KEY, token);
    if let Some(app) = app {
      self.ctx.set_extra(APP_KEY, app);
    }
    self.ctx.set_extra(TEAM_URL_KEY, &id.team_url);
    self.ctx.set_extra(TEAM_KEY, &id.team);
    self.ctx.set_extra(USER_KEY, &id.user_id);
  }

  /// The workspace's web address (`https://acme.slack.com/`), for permalinks.
  pub async fn team_url(&self) -> Option<String> {
    if self.env.is_none()
      && let Some(url) = self.ctx.extra(TEAM_URL_KEY).filter(|u| !u.is_empty())
    {
      return Some(url);
    }
    self.known().await.ok().map(|(_, url)| url)
  }

  /// The id of the account the token acts as.
  pub async fn me(&self) -> Result<String> {
    if self.env.is_none()
      && let Some(id) = self.ctx.extra(USER_KEY).filter(|u| !u.is_empty())
    {
      return Ok(id);
    }
    Ok(self.known().await?.0)
  }

  /// `auth.test` once per run (env tokens are never saved).
  async fn known(&self) -> Result<(String, String)> {
    if let Some(k) = self.ident.borrow().clone() {
      return Ok(k);
    }
    let id = self.identity().await?;
    let k = (id.user_id, id.team_url);
    *self.ident.borrow_mut() = Some(k.clone());
    if self.env.is_none() {
      self.ctx.set_extra(USER_KEY, &k.0);
      self.ctx.set_extra(TEAM_URL_KEY, &k.1);
    }
    Ok(k)
  }

  pub async fn identity(&self) -> Result<Identity> {
    let token = self.require()?;
    identity(&self.call_with(&token, "auth.test", Vec::new()).await?)
  }

  // ── calls ────────────────────────────────────────────────────────────

  /// A Web API method with the session's token.
  pub async fn call(&self, method: &str, params: Vec<(&str, String)>) -> Result<Value> {
    let token = self.require()?;
    self.call_with(&token, method, params).await
  }

  /// A write: the same, after a short human-like pause for user tokens.
  pub async fn write(&self, method: &str, params: Vec<(&str, String)>) -> Result<Value> {
    let token = self.require()?;
    if Kind::of(&token) == Kind::User {
      self
        .ctx
        .http
        .pause(Duration::from_millis(300), Duration::from_millis(900))
        .await;
    }
    self.call_with(&token, method, params).await
  }

  pub async fn call_with(
    &self,
    token: &str,
    method: &str,
    params: Vec<(&str, String)>,
  ) -> Result<Value> {
    let mut tries = 0;
    loop {
      tries += 1;
      let resp = self
        .ctx
        .http
        .post(format!("{API}/{method}"))
        .no_cookies()
        .retries(0)
        .header("authorization", format!("Bearer {token}"))
        .form(params.iter().map(|(k, v)| (*k, v.as_str())))
        .send()
        .await?;
      if resp.status.as_u16() == 429 {
        let wait = resp
          .header("retry-after")
          .and_then(|s| s.parse::<u64>().ok())
          .unwrap_or(5);
        if tries < MAX_TRIES && wait <= MAX_WAIT {
          tracing::debug!("{method}: rate limited, waiting {wait}s");
          tokio::time::sleep(Duration::from_secs(wait)).await;
          continue;
        }
        return Err(Error::new(
          ErrorCode::RateLimited,
          format!("Slack rate limit reached on {method} (retry in {wait}s)"),
        ));
      }
      if resp.status.is_server_error() && tries < MAX_TRIES {
        tokio::time::sleep(Duration::from_secs(u64::from(tries))).await;
        continue;
      }
      let v = resp.check()?.value()?;
      if v.bool("ok") == Some(true) {
        return Ok(v);
      }
      return Err(error(method, &v, self.env.is_some()));
    }
  }

  /// POST a file's bytes to an upload URL handed out by
  /// `files.getUploadURLExternal`.
  pub async fn upload(&self, url: &str, data: Vec<u8>) -> Result<()> {
    self
      .ctx
      .http
      .post(url)
      .no_cookies()
      .bytes(data, "application/octet-stream")
      .send()
      .await?
      .check()
      .map(drop)
  }
}

pub fn identity(v: &Value) -> Result<Identity> {
  Ok(Identity {
    user_id: v
      .str("user_id")
      .ok_or_else(|| Error::upstream("auth.test returned no user"))?,
    team: v.str("team").unwrap_or_default(),
    team_url: v.str("url").unwrap_or_default(),
  })
}

/// Slack's `{ok: false, error, needed}` as a shared error.
fn error(method: &str, v: &Value, from_env: bool) -> Error {
  let code = v.str("error").unwrap_or_else(|| "unknown_error".into());
  let text = format!("Slack {method}: {code}");
  match code.as_str() {
    "not_authed" | "invalid_auth" | "token_revoked" | "token_expired" | "account_inactive" => {
      Error::auth(text).with_hint(if from_env {
      "check SLACK_TOKEN"
    } else {
      "the token was revoked or the app uninstalled; log in again with a fresh token"
      })
    }
    "missing_scope" => {
      let needed = v.str("needed").unwrap_or_default();
      Error::new(
        ErrorCode::PermissionDenied,
        format!("{text} (needs {needed})"),
      )
      .with_hint(format!(
        "add the `{needed}` scope to the Slack app, reinstall it and use the new token"
      ))
    }
    "not_allowed_token_type" => Error::new(ErrorCode::UnsupportedOperation, text)
      .with_hint("this method needs the other kind of token (user xoxp-… or bot xoxb-…)"),
    "channel_not_found" | "message_not_found" | "thread_not_found" | "user_not_found"
    | "users_not_found" | "file_not_found" | "bookmark_not_found" | "team_not_found" => {
      Error::not_found(text)
    }
    "not_in_channel" | "is_archived" => Error::new(ErrorCode::PermissionDenied, text)
      .with_hint("invite the app to the channel (`/invite @app`), or `social slack join CHANNEL` for public ones"),
    "cant_delete_message" | "cant_update_message" | "edit_window_closed" | "restricted_action"
    | "no_permission" | "access_denied" | "ekm_access_denied" | "method_deprecated" => {
      Error::new(ErrorCode::PermissionDenied, text)
    }
    "ratelimited" => Error::new(ErrorCode::RateLimited, text),
    _ if code.starts_with("invalid_") || code.starts_with("too_") || code.starts_with("no_")
      || code.starts_with("already_") || code.starts_with("not_") || code == "msg_too_long" =>
    {
      Error::input(text)
    }
    _ => Error::upstream(text),
  }
}
