//! Telegram references: chats and messages in the forms people paste.
//!
//! Chats: `@name`, `name`, `t.me/name` (also `t.me/s/name`, telegram.me,
//! `tg://resolve?domain=name`), `t.me/c/<id>` (private supergroups and
//! channels), Bot API ids (`123` users, `-123` basic groups, `-100…`
//! supergroups and channels), invite links (`t.me/+…`, `t.me/joinchat/…`)
//! and `me` / `self` (your own account: Saved Messages).
//!
//! Messages: `t.me/name/42`, `t.me/s/name/42`, `t.me/c/<id>/42`, forum links
//! with the topic before the message id, `?comment=7` for a comment under a
//! channel post, `tg://privatepost?channel=…&post=…`, and `CHAT/42` with any
//! chat form (the `id` messages carry when there is no link).

use atp_core::chat;
use atp_core::{Error, Result};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chat {
  Username(String),
  /// Bot API dialog id: users positive, basic groups negative, supergroups
  /// and channels `-100…`.
  Id(i64),
  /// Hash of an invite link.
  Invite(String),
  /// Your own account (`me`, `self`): Saved Messages for a user account.
  Me,
}

impl Chat {
  pub fn username(&self) -> Option<&str> {
    match self {
      Chat::Username(u) => Some(u),
      _ => None,
    }
  }
}

impl std::fmt::Display for Chat {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Chat::Username(u) => write!(f, "@{u}"),
      Chat::Id(id) => write!(f, "{id}"),
      Chat::Invite(hash) => write!(f, "https://t.me/+{hash}"),
      Chat::Me => f.write_str("me"),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
  pub chat: Chat,
  pub id: i32,
  /// A comment under this (channel) message, from `?comment=`.
  pub comment: Option<i32>,
}

pub fn chat(arg: &str) -> Result<Chat> {
  let arg = arg.trim();
  if let Some(link) = Link::parse(arg) {
    return link_chat(&link).ok_or_else(|| bad_chat(arg));
  }
  plain_chat(arg).ok_or_else(|| bad_chat(arg))
}

pub fn message(arg: &str) -> Result<Msg> {
  let arg = arg.trim();
  if let Some(link) = Link::parse(arg) {
    return link_message(&link).ok_or_else(|| bad_message(arg));
  }
  let (chat, id) = chat::split_message_id(arg).ok_or_else(|| bad_message(arg))?;
  let id = id.parse().map_err(|_| bad_message(arg))?;
  Ok(Msg {
    chat: plain_chat(chat).ok_or_else(|| bad_message(arg))?,
    id,
    comment: None,
  })
}

/// `@name`, `name`, a Bot API id, or `me` / `self`.
fn plain_chat(arg: &str) -> Option<Chat> {
  let arg = arg.strip_prefix('@').unwrap_or(arg);
  if let Ok(id) = arg.parse::<i64>() {
    return Some(Chat::Id(id));
  }
  if ["me", "self"].iter().any(|me| arg.eq_ignore_ascii_case(me)) {
    return Some(Chat::Me);
  }
  is_username(arg).then(|| Chat::Username(arg.to_owned()))
}

pub fn is_username(name: &str) -> bool {
  let mut chars = name.chars();
  chars.next().is_some_and(|c| c.is_ascii_alphabetic())
    && (3..=32).contains(&name.len())
    && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Bot API id of a private supergroup / channel from its `t.me/c/<id>` form.
fn channel_id(bare: &str) -> Option<i64> {
  let bare: i64 = bare.parse().ok()?;
  Some(-1_000_000_000_000 - bare)
}

struct Link {
  segments: Vec<String>,
  query: Vec<(String, String)>,
  /// `tg://` links name their kind in the host.
  tg: Option<String>,
}

impl Link {
  fn parse(arg: &str) -> Option<Self> {
    let with_scheme = if arg.contains("://") {
      arg.to_owned()
    } else if ["t.me/", "telegram.me/", "telegram.dog/"]
      .iter()
      .any(|h| arg.starts_with(h))
    {
      format!("https://{arg}")
    } else {
      return None;
    };
    let url = Url::parse(&with_scheme).ok()?;
    let query = url
      .query_pairs()
      .map(|(k, v)| (k.into_owned(), v.into_owned()))
      .collect();
    if url.scheme() == "tg" {
      return Some(Self {
        segments: Vec::new(),
        query,
        tg: url.host_str().map(str::to_owned),
      });
    }
    let host = url.host_str()?.trim_start_matches("www.");
    if !["t.me", "telegram.me", "telegram.dog"].contains(&host) {
      return None;
    }
    let segments = url
      .path_segments()?
      .filter(|s| !s.is_empty())
      .map(str::to_owned)
      .collect();
    Some(Self {
      segments,
      query,
      tg: None,
    })
  }

  fn query(&self, key: &str) -> Option<&str> {
    self
      .query
      .iter()
      .find(|(k, _)| k == key)
      .map(|(_, v)| v.as_str())
  }

  /// Path without the `s/` of web previews.
  fn path(&self) -> &[String] {
    match self.segments.first().map(String::as_str) {
      Some("s") => &self.segments[1..],
      _ => &self.segments,
    }
  }
}

fn link_chat(link: &Link) -> Option<Chat> {
  match link.tg.as_deref() {
    Some("resolve") => return plain_chat(link.query("domain")?),
    Some("privatepost") => return channel_id(link.query("channel")?).map(Chat::Id),
    Some("join") => return Some(Chat::Invite(link.query("invite")?.to_owned())),
    Some(_) => return None,
    None => {}
  }
  let path = link.path();
  match path.first()?.as_str() {
    "c" => channel_id(path.get(1)?).map(Chat::Id),
    "joinchat" => Some(Chat::Invite(path.get(1)?.clone())),
    first if first.starts_with('+') => Some(Chat::Invite(first[1..].to_owned())),
    first => is_username(first).then(|| Chat::Username(first.to_owned())),
  }
}

fn link_message(link: &Link) -> Option<Msg> {
  let comment = link.query("comment").and_then(|c| c.parse().ok());
  let chat = link_chat(link)?;
  let id: i32 = match link.tg.as_deref() {
    Some(_) => link.query("post")?.parse().ok()?,
    // The message id comes last: `name/42`, `name/<topic>/42`, `c/<id>/42`.
    None => link.path().last()?.parse().ok()?,
  };
  let first_is_id = match link.path().first().map(String::as_str) {
    Some("c") => link.path().len() >= 3,
    _ => link.path().len() >= 2,
  };
  first_is_id.then_some(Msg { chat, id, comment })
}

fn bad_chat(arg: &str) -> Error {
  Error::input(format!(
    "not a Telegram chat: `{arg}` (expected @name, a t.me link, a numeric id or `me`)"
  ))
}

fn bad_message(arg: &str) -> Error {
  Error::input(format!(
    "not a Telegram message: `{arg}` (expected a t.me/name/42 link or CHAT/42)"
  ))
}

/// What other commands accept back for a message: its link when it has one.
pub fn message_url(username: Option<&str>, dialog_id: i64, id: i32) -> Option<String> {
  if let Some(u) = username {
    return Some(format!("https://t.me/{u}/{id}"));
  }
  // Private supergroups and channels have member-only links.
  (dialog_id < -1_000_000_000_000).then(|| {
    let bare = -1_000_000_000_000 - dialog_id;
    format!("https://t.me/c/{bare}/{id}")
  })
}

/// `CHAT/MESSAGE`, with the username when the chat has one.
pub fn message_id(username: Option<&str>, dialog_id: i64, id: i32) -> String {
  let chat = username.map_or_else(|| dialog_id.to_string(), str::to_owned);
  chat::message_id(&chat, &id.to_string())
}
