//! Bot accounts through the Bot API (`login --token 123:ABC`, or the
//! `TELEGRAM_BOT_TOKEN` variable). Bots can send, edit, delete, react, pin,
//! look up chats and admins and receive updates, but cannot list their chats
//! or read history: Telegram only hands them messages as updates.

mod files;
mod live;
mod messages;
mod parse;
mod users;

use std::time::Duration;

use atp_core::credentials;
use atp_core::http::Part;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

use crate::refs::{self, Chat};

const API: &str = "https://api.telegram.org";
/// Session extra holding the token.
pub const TOKEN: &str = "bot_token";

pub struct Bot {
  token: String,
}

impl Bot {
  pub fn from_ctx(ctx: &Ctx) -> Option<Self> {
    let token = ctx
      .extra(TOKEN)
      .filter(|t| !t.is_empty())
      .or_else(|| credentials::env("TELEGRAM_BOT_TOKEN"))?;
    Some(Self {
      token: token.trim().to_owned(),
    })
  }

  /// `123456:ABC…`, as @BotFather hands it out.
  pub fn valid_token(token: &str) -> bool {
    token.split_once(':').is_some_and(|(id, secret)| {
      !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) && secret.len() >= 20
    })
  }

  pub(super) async fn call(&self, ctx: &Ctx, method: &str, body: Value) -> Result<Value> {
    let resp = ctx
      .http
      .post(format!("{API}/bot{}/{method}", self.token))
      .no_cookies()
      .json(&body)
      .send()
      .await?;
    answer(resp.value().unwrap_or_default(), resp.status.as_u16())
  }

  pub(super) async fn call_long(
    &self,
    ctx: &Ctx,
    method: &str,
    body: Value,
    wait: Duration,
  ) -> Result<Value> {
    let resp = ctx
      .http
      .post(format!("{API}/bot{}/{method}", self.token))
      .no_cookies()
      .no_throttle()
      .timeout(wait)
      .json(&body)
      .send()
      .await?;
    answer(resp.value().unwrap_or_default(), resp.status.as_u16())
  }

  pub(super) async fn call_parts(
    &self,
    ctx: &Ctx,
    method: &str,
    parts: Vec<Part>,
  ) -> Result<Value> {
    let resp = ctx
      .http
      .post(format!("{API}/bot{}/{method}", self.token))
      .no_cookies()
      .timeout(Duration::from_secs(300))
      .multipart(parts)
      .send()
      .await?;
    answer(resp.value().unwrap_or_default(), resp.status.as_u16())
  }
}

fn answer(v: Value, status: u16) -> Result<Value> {
  if v.bool("ok") == Some(true) {
    return Ok(v.get("result").cloned().unwrap_or(Value::Null));
  }
  let description = v
    .str("description")
    .unwrap_or_else(|| format!("HTTP {status}"));
  let text = format!("Telegram Bot API: {description}");
  Err(match v.i64("error_code").unwrap_or(status.into()) {
    401 => Error::auth(text).with_hint("the bot token was refused; get a new one from @BotFather"),
    403 => Error::new(ErrorCode::PermissionDenied, text),
    404 => Error::not_found(text),
    429 => Error::new(
      ErrorCode::RateLimited,
      format!(
        "{text} (retry after {} s)",
        v.i64("parameters.retry_after").unwrap_or(1)
      ),
    ),
    400 if description.contains("not found") => Error::not_found(text),
    400 => Error::input(text),
    _ => Error::upstream(text),
  })
}

/// `chat_id` for the Bot API: `@name` or the numeric id.
pub(super) fn chat_id(chat: &Chat) -> Result<Value> {
  match chat {
    Chat::Username(u) => Ok(format!("@{u}").into()),
    Chat::Id(id) => Ok((*id).into()),
    Chat::Invite(_) => Err(Error::input(
      "bots cannot use invite links; use the chat's id or @name",
    )),
    Chat::Me => Err(Error::input(
      "`me` is a user account's Saved Messages, which bots do not have; use a chat's id or @name",
    )),
  }
}

pub(super) fn message_ref(arg: &str) -> Result<(Value, i32)> {
  let m = refs::message(arg)?;
  Ok((chat_id(&m.chat)?, m.id))
}

pub(super) fn same_chat(want: &Chat, chat: &Value) -> bool {
  match want {
    Chat::Id(id) => chat.i64("id") == Some(*id),
    Chat::Username(u) => chat
      .str("username")
      .is_some_and(|n| n.eq_ignore_ascii_case(u)),
    Chat::Invite(_) | Chat::Me => false,
  }
}
