//! Servers are `guild:<id>`, direct chats `user:<id>` (opened on use), and
//! anything else is a channel id as the server names it (these can hold
//! colons, e.g. `private:123`). Messages are `CHANNEL/MESSAGE`.

use social_core::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chat {
  Guild(String),
  Channel(String),
  User(String),
}

pub fn chat(arg: &str) -> Result<Chat> {
  let arg = arg.trim();
  if arg.is_empty() {
    return Err(Error::input("empty chat"));
  }
  Ok(if let Some(g) = arg.strip_prefix("guild:") {
    Chat::Guild(g.to_owned())
  } else if let Some(u) = arg.strip_prefix("user:") {
    Chat::User(u.to_owned())
  } else {
    Chat::Channel(arg.to_owned())
  })
}

#[derive(Debug, Clone)]
pub struct MessageRef {
  pub channel: String,
  pub id: String,
}

pub fn message(arg: &str) -> Result<MessageRef> {
  match arg.trim().rsplit_once('/') {
    Some((channel, id)) if !channel.is_empty() && !id.is_empty() => Ok(MessageRef {
      channel: channel.to_owned(),
      id: id.to_owned(),
    }),
    _ => Err(Error::input(format!(
      "not a Satori message: {arg}; use CHANNEL/MESSAGE as listed by `messages`"
    ))),
  }
}
