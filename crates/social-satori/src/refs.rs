//! Servers are `guild:<id>`, direct chats `user:<id>` (opened on use), and
//! anything else is a channel id as the server names it (these can hold
//! colons, e.g. `private:123`). Messages are `CHANNEL/MESSAGE`.

use social_core::chat;
use social_core::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRef {
  Guild(String),
  Channel(String),
  User(String),
}

pub fn chat(arg: &str) -> Result<ChatRef> {
  let arg = arg.trim();
  if arg.is_empty() {
    return Err(Error::input("empty chat"));
  }
  Ok(if let Some(g) = arg.strip_prefix("guild:") {
    ChatRef::Guild(g.to_owned())
  } else if let Some(u) = arg.strip_prefix("user:") {
    ChatRef::User(u.to_owned())
  } else {
    ChatRef::Channel(arg.to_owned())
  })
}

#[derive(Debug, Clone)]
pub struct MessageRef {
  pub channel: String,
  pub id: String,
}

pub fn message(arg: &str) -> Result<MessageRef> {
  match chat::split_message_id(arg) {
    Some((channel, id)) => Ok(MessageRef {
      channel: channel.to_owned(),
      id: id.to_owned(),
    }),
    None => Err(Error::input(format!(
      "not a Satori message: {arg}; use CHANNEL/MESSAGE as listed by `messages`"
    ))),
  }
}
