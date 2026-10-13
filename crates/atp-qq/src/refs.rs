//! Chats: `guild:<id>` (a 频道 server), a sub-channel's id (or
//! `channel:<id>`), `dm:<guild id>` (a 频道私信 session), `group:<openid>`
//! and `user:<openid>` (群聊 and 单聊, known only from events). Messages are
//! `CHAT/<message id>`.

use atp_core::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chat {
  Guild(String),
  Channel(String),
  Dm(String),
  Group(String),
  User(String),
}

impl Chat {
  pub fn id(&self) -> String {
    match self {
      Chat::Guild(g) => format!("guild:{g}"),
      Chat::Channel(c) => c.clone(),
      Chat::Dm(g) => format!("dm:{g}"),
      Chat::Group(g) => format!("group:{g}"),
      Chat::User(u) => format!("user:{u}"),
    }
  }
}

pub fn chat(arg: &str) -> Result<Chat> {
  let arg = arg.trim();
  let (kind, id) = arg.split_once(':').unwrap_or(("channel", arg));
  if id.is_empty() || id.contains('/') {
    return Err(bad(arg));
  }
  Ok(match kind {
    "guild" | "server" => Chat::Guild(id.to_owned()),
    "channel" => Chat::Channel(id.to_owned()),
    "dm" => Chat::Dm(id.to_owned()),
    "group" => Chat::Group(id.to_owned()),
    "user" | "c2c" => Chat::User(id.to_owned()),
    _ => return Err(bad(arg)),
  })
}

fn bad(arg: &str) -> Error {
  Error::input(format!(
    "not a QQ chat: {arg}; use a channel id, guild:<id>, dm:<guild id>, group:<openid> or user:<openid>"
  ))
}

#[derive(Debug, Clone)]
pub struct MessageRef {
  pub chat: Chat,
  pub id: String,
}

pub fn message(arg: &str) -> Result<MessageRef> {
  let arg = arg.trim();
  match arg.split_once('/') {
    Some((c, id)) if !id.is_empty() => Ok(MessageRef {
      chat: chat(c)?,
      id: id.to_owned(),
    }),
    _ => Err(Error::input(format!(
      "not a QQ message: {arg}; use CHAT/MESSAGE (as `listen` prints them)"
    ))),
  }
}
