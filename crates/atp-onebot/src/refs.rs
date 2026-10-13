//! Chats are `group:<id>` or `user:<id>` (also `g:` / `private:` / `friend:`
//! / `u:`); messages are `CHAT/<message_id>`, or a bare message id where
//! the implementation can look it up alone (`get_msg`).

use atp_core::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chat {
  Group(String),
  Private(String),
}

impl Chat {
  pub fn id(&self) -> String {
    match self {
      Chat::Group(g) => format!("group:{g}"),
      Chat::Private(u) => format!("user:{u}"),
    }
  }
}

pub fn chat(arg: &str) -> Result<Chat> {
  let arg = arg.trim();
  let (kind, id) = arg.split_once(':').ok_or_else(|| bad_chat(arg))?;
  if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
    return Err(bad_chat(arg));
  }
  match kind {
    "group" | "g" => Ok(Chat::Group(id.to_owned())),
    "user" | "u" | "private" | "friend" => Ok(Chat::Private(id.to_owned())),
    _ => Err(bad_chat(arg)),
  }
}

fn bad_chat(arg: &str) -> Error {
  Error::input(format!(
    "not a OneBot chat: {arg}; use group:<group number> or user:<QQ number> (or #N after `chats`)"
  ))
}

#[derive(Debug, Clone)]
pub struct MessageRef {
  pub chat: Option<Chat>,
  pub id: String,
}

pub fn message(arg: &str) -> Result<MessageRef> {
  let arg = arg.trim();
  if let Some((chat_part, id)) = arg.rsplit_once('/') {
    return Ok(MessageRef {
      chat: Some(chat(chat_part)?),
      id: id.to_owned(),
    });
  }
  let digits = arg.strip_prefix('-').unwrap_or(arg);
  if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(MessageRef {
      chat: None,
      id: arg.to_owned(),
    });
  }
  Err(Error::input(format!(
    "not a OneBot message: {arg}; use CHAT/MESSAGE_ID (as listed by `messages`) or a message id"
  )))
}

/// User ids are QQ numbers; `user:` prefixes are accepted too.
pub fn user(arg: &str) -> Result<String> {
  let id = arg
    .trim()
    .trim_start_matches("user:")
    .trim_start_matches("u:");
  if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
    Ok(id.to_owned())
  } else {
    Err(Error::input(format!("not a QQ number: {arg}")))
  }
}

/// Numbers go out as JSON numbers (most implementations insist).
pub fn num(id: &str) -> serde_json::Value {
  id.parse::<i64>()
    .map(serde_json::Value::from)
    .unwrap_or_else(|_| serde_json::Value::from(id))
}
