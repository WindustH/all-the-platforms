//! Notes, users, chats and chat messages as arguments.
//!
//! - notes: `https://<instance>/notes/<id>` or the id; links to notes of
//!   other servers are looked up through the instance (`ap/show`);
//! - users: `@name`, `@name@host`, a profile link (`/@name`), an id, `me`;
//! - chats: `user:<id>` (direct chat), `room:<id>`, or a user as above;
//! - chat messages: `user:<id>/<message>` or `room:<id>/<message>` (the `id`
//!   of listed chat messages).

use social_core::{Error, Result};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserRef {
  Me,
  Id(String),
  Name {
    username: String,
    host: Option<String>,
  },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteRef {
  Id(String),
  /// A link to another server, resolved with `ap/show`.
  Remote(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chat {
  User(UserRef),
  Room(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
  pub chat: Chat,
  pub id: String,
}

fn is_id(s: &str) -> bool {
  !s.is_empty() && s.len() <= 32 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub fn note(arg: &str, instance: &str) -> Result<NoteRef> {
  let arg = arg.trim();
  if let Ok(url) = Url::parse(arg) {
    let local = url.host_str() == Some(instance);
    if local && let Some(id) = url.path().strip_prefix("/notes/") {
      let id = id.trim_end_matches('/');
      if is_id(id) {
        return Ok(NoteRef::Id(id.to_owned()));
      }
    }
    return Ok(NoteRef::Remote(arg.to_owned()));
  }
  let id = arg.strip_prefix("notes/").unwrap_or(arg);
  if is_id(id) {
    return Ok(NoteRef::Id(id.to_owned()));
  }
  Err(Error::input(format!(
    "not a note: {arg} (expected a /notes/ link or a note id)"
  )))
}

pub fn user(arg: &str, instance: &str) -> Result<UserRef> {
  let arg = arg.trim();
  if arg == "me" {
    return Ok(UserRef::Me);
  }
  if let Ok(url) = Url::parse(arg) {
    let path = url.path().trim_matches('/');
    if let Some(name) = path.strip_prefix('@') {
      let (username, host) = split_handle(name);
      let host = host.or_else(|| url.host_str().filter(|h| *h != instance).map(str::to_owned));
      return Ok(UserRef::Name { username, host });
    }
    return Err(Error::input(format!("not a profile link: {arg}")));
  }
  if let Some(name) = arg.strip_prefix('@') {
    let (username, host) = split_handle(name);
    let host = host.filter(|h| h != instance);
    return Ok(UserRef::Name { username, host });
  }
  if let Some(id) = arg.strip_prefix("user:") {
    return Ok(UserRef::Id(id.to_owned()));
  }
  if is_id(arg) {
    return Ok(UserRef::Id(arg.to_owned()));
  }
  Err(Error::input(format!(
    "not a user: {arg} (expected @name, @name@host, a profile link or an id)"
  )))
}

fn split_handle(s: &str) -> (String, Option<String>) {
  match s.split_once('@') {
    Some((u, h)) if !h.is_empty() => (u.to_owned(), Some(h.to_ascii_lowercase())),
    _ => (s.trim_end_matches('@').to_owned(), None),
  }
}

pub fn chat(arg: &str, instance: &str) -> Result<Chat> {
  let arg = arg.trim();
  if let Some(id) = arg.strip_prefix("room:") {
    return Ok(Chat::Room(id.to_owned()));
  }
  if let Ok(url) = Url::parse(arg)
    && let Some(id) = url.path().strip_prefix("/chat/room/")
  {
    return Ok(Chat::Room(id.trim_end_matches('/').to_owned()));
  }
  user(arg, instance).map(Chat::User)
}

/// A chat message (`user:<id>/<message>` / `room:<id>/<message>`), or `None`
/// for anything else (then it is a note).
pub fn chat_message(arg: &str) -> Option<ChatMessage> {
  let arg = arg.trim();
  let (chat, id) = arg.rsplit_once('/')?;
  if !is_id(id) {
    return None;
  }
  let chat = if let Some(uid) = chat.strip_prefix("user:") {
    Chat::User(UserRef::Id(uid.to_owned()))
  } else {
    Chat::Room(chat.strip_prefix("room:")?.to_owned())
  };
  Some(ChatMessage {
    chat,
    id: id.to_owned(),
  })
}
