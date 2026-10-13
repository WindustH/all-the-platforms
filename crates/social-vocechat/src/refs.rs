//! Chats and messages as arguments: `group:<gid>` (a channel), `user:<uid>`
//! (a direct chat), the web app's `#/chat/channel/<gid>` / `#/chat/dm/<uid>`
//! links; messages as `<chat>/<mid>` (the `id` of listed messages).

use social_core::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chat {
  Group(i64),
  User(i64),
}

impl Chat {
  pub fn key(self) -> String {
    match self {
      Chat::Group(g) => format!("group:{g}"),
      Chat::User(u) => format!("user:{u}"),
    }
  }

  /// The API path of the chat (`/group/5`, `/user/3`).
  pub fn path(self) -> String {
    match self {
      Chat::Group(g) => format!("/group/{g}"),
      Chat::User(u) => format!("/user/{u}"),
    }
  }
}

fn number(s: &str) -> Option<i64> {
  s.trim().trim_end_matches('/').parse().ok()
}

pub fn chat(arg: &str) -> Result<Chat> {
  let arg = arg.trim();
  let tail = arg.rsplit_once("#/chat/").map_or(arg, |(_, t)| t);
  let parsed = if let Some(g) = tail
    .strip_prefix("group:")
    .or_else(|| tail.strip_prefix("channel/"))
  {
    number(g).map(Chat::Group)
  } else if let Some(u) = tail
    .strip_prefix("user:")
    .or_else(|| tail.strip_prefix("dm/"))
  {
    number(u).map(Chat::User)
  } else {
    None
  };
  parsed.ok_or_else(|| {
    Error::input(format!(
      "not a chat: {arg} (expected group:<id>, user:<id> or a web app chat link)"
    ))
  })
}

pub fn message(arg: &str) -> Result<(Chat, i64)> {
  let arg = arg.trim();
  let (c, m) = arg.rsplit_once('/').ok_or_else(|| {
    Error::input(format!(
      "not a message: {arg} (expected group:<id>/<mid> or user:<id>/<mid>)"
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
  let s = arg.strip_prefix("user:").unwrap_or(arg);
  number(s)
    .map(Some)
    .ok_or_else(|| Error::input(format!("not a user: {arg} (expected user:<id> or me)")))
}
