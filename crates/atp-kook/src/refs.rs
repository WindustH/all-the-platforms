//! Servers are `guild:<id>`, channels their bare id, direct chats
//! `dm:<user id>`; messages are `CHAT/<message id>` (message ids are UUIDs).

use atp_core::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chat {
  Guild(String),
  Channel(String),
  Direct(String),
}

impl Chat {
  pub fn id(&self) -> String {
    match self {
      Chat::Guild(g) => format!("guild:{g}"),
      Chat::Channel(c) => c.clone(),
      Chat::Direct(u) => format!("dm:{u}"),
    }
  }
}

pub fn chat(arg: &str) -> Result<Chat> {
  let arg = arg.trim();
  let (kind, id) = match arg.split_once(':') {
    Some((k, id)) => (k, id),
    None => ("channel", arg),
  };
  if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
    return Err(Error::input(format!(
      "not a KOOK chat: {arg}; use a channel id, guild:<id> or dm:<user id> (or #N after `chats`)"
    )));
  }
  Ok(match kind {
    "guild" | "server" => Chat::Guild(id.to_owned()),
    "dm" | "user" => Chat::Direct(id.to_owned()),
    _ => Chat::Channel(id.to_owned()),
  })
}

#[derive(Debug, Clone)]
pub struct MessageRef {
  pub chat: Option<Chat>,
  pub id: String,
}

pub fn message(arg: &str) -> Result<MessageRef> {
  let arg = arg.trim();
  if let Some((c, id)) = arg.rsplit_once('/') {
    return Ok(MessageRef {
      chat: Some(chat(c)?),
      id: id.to_owned(),
    });
  }
  if arg.len() >= 32 && arg.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
    return Ok(MessageRef {
      chat: None,
      id: arg.to_owned(),
    });
  }
  Err(Error::input(format!(
    "not a KOOK message: {arg}; use CHAT/MESSAGE as listed by `messages`"
  )))
}

pub fn user(arg: &str) -> Result<String> {
  let id = arg
    .trim()
    .trim_start_matches("user:")
    .trim_start_matches("dm:");
  if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
    Ok(id.to_owned())
  } else {
    Err(Error::input(format!("not a KOOK user id: {arg}")))
  }
}
