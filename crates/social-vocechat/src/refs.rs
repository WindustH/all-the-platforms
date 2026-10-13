//! Chats and messages as arguments: `group:<gid>` (a channel), `user:<uid>`
//! (a direct chat), the web app's `#/chat/channel/<gid>` / `#/chat/dm/<uid>`
//! links; messages as `<chat>/<mid>` (the `id` of listed messages).

use social_core::chat;
use social_core::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRef {
  Group(i64),
  User(i64),
}

impl ChatRef {
  pub fn key(self) -> String {
    match self {
      ChatRef::Group(g) => format!("group:{g}"),
      ChatRef::User(u) => format!("user:{u}"),
    }
  }

  /// The API path of the chat (`/group/5`, `/user/3`).
  pub fn path(self) -> String {
    match self {
      ChatRef::Group(g) => format!("/group/{g}"),
      ChatRef::User(u) => format!("/user/{u}"),
    }
  }
}

fn number(text: &str) -> Option<i64> {
  text.trim().trim_end_matches('/').parse().ok()
}

pub fn chat(arg: &str) -> Result<ChatRef> {
  let arg = arg.trim();
  let tail = arg.rsplit_once("#/chat/").map_or(arg, |(_, t)| t);
  let parsed = if let Some(g) = tail
    .strip_prefix("group:")
    .or_else(|| tail.strip_prefix("channel/"))
  {
    number(g).map(ChatRef::Group)
  } else if let Some(u) = tail
    .strip_prefix("user:")
    .or_else(|| tail.strip_prefix("dm/"))
  {
    number(u).map(ChatRef::User)
  } else {
    None
  };
  parsed.ok_or_else(|| {
    Error::input(format!(
      "not a chat: {arg} (expected group:<id>, user:<id> or a web app chat link)"
    ))
  })
}

/// The chat and number of a `<chat>/<mid>` message id.
pub fn message_id(arg: &str) -> Result<(ChatRef, i64)> {
  let (c, m) = chat::split_message_id(arg).ok_or_else(|| {
    Error::input(format!(
      "not a message: {} (expected group:<id>/<mid> or user:<id>/<mid>)",
      arg.trim()
    ))
  })?;
  let mid = number(m).ok_or_else(|| Error::input(format!("not a message id: {m}")))?;
  Ok((chat(c)?, mid))
}

pub fn user(arg: &str) -> Result<Option<i64>> {
  let arg = arg.trim();
  if arg == "me" {
    return Ok(None);
  }
  let id = arg.strip_prefix("user:").unwrap_or(arg);
  number(id)
    .map(Some)
    .ok_or_else(|| Error::input(format!("not a user: {arg} (expected user:<id> or me)")))
}
