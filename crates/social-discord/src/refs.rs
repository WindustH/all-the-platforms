//! What commands accept: message links (`discord.com/channels/G/C/M`, also
//! ptb / canary / discordapp.com), `CHANNEL/MESSAGE` and Discord's own
//! "Copy ID" form `CHANNEL-MESSAGE`; channel and server links, `<#id>`;
//! users as ids, `<@id>`, `user:id` or `discord.com/users/id`.

use std::sync::OnceLock;

use regex::Regex;
use social_core::{Error, Result};

/// A message and the channel it is in (and its guild, when the link says).
#[derive(Debug, Clone)]
pub struct MessageRef {
  pub channel: String,
  pub message: String,
  pub guild: Option<String>,
}

/// A chat argument: what it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chat {
  /// A bare id: a channel or a server, the API tells which.
  Id(String),
  Channel(String),
  Server(String),
  /// A user: their direct chat.
  User(String),
}

fn link() -> &'static Regex {
  static RE: OnceLock<Regex> = OnceLock::new();
  RE.get_or_init(|| {
    Regex::new(
      r"(?:https?://)?(?:(?:ptb|canary|www)\.)?discord(?:app)?\.com/channels/(@me|\d+)(?:/(\d+))?(?:/(\d+))?",
    )
    .expect("valid regex")
  })
}

fn user_link() -> &'static Regex {
  static RE: OnceLock<Regex> = OnceLock::new();
  RE.get_or_init(|| {
    Regex::new(r"(?:https?://)?(?:(?:ptb|canary|www)\.)?discord(?:app)?\.com/users/(\d+)")
      .expect("valid regex")
  })
}

pub fn is_snowflake(s: &str) -> bool {
  (15..=21).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

pub fn message(arg: &str) -> Result<MessageRef> {
  let arg = arg.trim();
  if let Some(c) = link().captures(arg)
    && let (Some(channel), Some(message)) = (c.get(2), c.get(3))
  {
    return Ok(MessageRef {
      guild: Some(c[1].to_owned()),
      channel: channel.as_str().to_owned(),
      message: message.as_str().to_owned(),
    });
  }
  for sep in ['/', '-'] {
    if let Some((channel, message)) = arg.split_once(sep)
      && is_snowflake(channel)
      && is_snowflake(message)
    {
      return Ok(MessageRef {
        channel: channel.to_owned(),
        message: message.to_owned(),
        guild: None,
      });
    }
  }
  Err(Error::input(format!(
    "not a Discord message: {arg}; give a message link or CHANNEL/MESSAGE ids"
  )))
}

pub fn user(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if let Some(c) = user_link().captures(arg) {
    return Ok(c[1].to_owned());
  }
  let id = arg
    .trim_start_matches("user:")
    .trim_start_matches("<@")
    .trim_start_matches('!')
    .trim_start_matches('@')
    .trim_end_matches('>');
  if is_snowflake(id) {
    return Ok(id.to_owned());
  }
  Err(Error::input(format!(
    "not a Discord user id: {arg}; Discord looks users up by id (`user:ID`, `<@ID>` or a profile link)"
  )))
}

pub fn chat(arg: &str) -> Result<Chat> {
  let arg = arg.trim();
  if let Some(c) = link().captures(arg) {
    return Ok(match c.get(2) {
      Some(channel) => Chat::Channel(channel.as_str().to_owned()),
      None if &c[1] == "@me" => {
        return Err(Error::input(
          "a link to @me names no chat; `social discord chats` lists them",
        ));
      }
      None => Chat::Server(c[1].to_owned()),
    });
  }
  if arg.starts_with("<@")
    || arg.starts_with('@')
    || arg.starts_with("user:")
    || user_link().is_match(arg)
  {
    return user(arg).map(Chat::User);
  }
  if let Some(id) = arg.strip_prefix("<#").and_then(|s| s.strip_suffix('>')) {
    return Ok(Chat::Channel(id.to_owned()));
  }
  for (prefix, wrap) in [
    ("server:", Chat::Server as fn(String) -> Chat),
    ("guild:", Chat::Server),
    ("channel:", Chat::Channel),
  ] {
    if let Some(id) = arg.strip_prefix(prefix)
      && is_snowflake(id)
    {
      return Ok(wrap(id.to_owned()));
    }
  }
  if is_snowflake(arg) {
    return Ok(Chat::Id(arg.to_owned()));
  }
  Err(Error::input(format!(
    "not a Discord chat: {arg}; give a channel or server id or link (`social discord chats` lists them)"
  )))
}
