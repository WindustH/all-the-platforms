//! Direct messages: conversations as chats, timeline entries as posts.

use atp_core::chat::message_id;
use atp_core::text::from_millis;
use atp_core::{Collection, Post, User, Value, ValueExt};

use super::media::attachment;
use crate::refs::user_url;

/// One conversation as a chat: its name (a group's, or the other people's).
pub fn chat(id: &str, c: &Value, v: &Value, me: &str) -> Collection {
  let others: Vec<User> = c
    .list("participants")
    .iter()
    .filter_map(|p| p.str("user_id"))
    .filter(|u| u != me)
    .filter_map(|u| participant(v, &u))
    .collect();
  let group = c.str("type").as_deref() == Some("GROUP_DM");
  let name = c.str("name").unwrap_or_else(|| {
    others
      .iter()
      .map(|u| u.name.clone())
      .collect::<Vec<_>>()
      .join(", ")
  });
  let mut chat = Collection {
    id: id.to_owned(),
    kind: if group { "group" } else { "dm" }.into(),
    name,
    url: Some(format!("https://x.com/messages/{id}")),
    items: Some(c.list("participants").len() as u64),
    raw: Some(c.clone()),
    ..Collection::default()
  };
  let handles: Vec<Value> = others
    .iter()
    .filter_map(|u| u.handle.clone().map(Value::from))
    .collect();
  chat.extra.insert("participants".into(), handles.into());
  if let Some(t) = c.i64("sort_timestamp").and_then(from_millis) {
    chat
      .extra
      .insert("last_activity".into(), t.to_string().into());
  }
  let read = c
    .list("participants")
    .iter()
    .find(|p| p.str("user_id").as_deref() == Some(me))
    .and_then(|p| p.str("last_read_event_id"));
  if let (Some(read), Some(max)) = (
    read,
    c.str("max_entry_id").or_else(|| c.str("sort_event_id")),
  ) {
    chat
      .extra
      .insert("unread".into(), (cmp_ids(&max, &read) > 0).into());
  }
  for key in ["muted", "trusted", "read_only"] {
    if let Some(b) = c.bool(key) {
      chat.extra.insert(key.into(), b.into());
    }
  }
  chat
}

/// Compare decimal ids of any length.
fn cmp_ids(a: &str, b: &str) -> i32 {
  match a.len().cmp(&b.len()).then(a.cmp(b)) {
    std::cmp::Ordering::Greater => 1,
    std::cmp::Ordering::Less => -1,
    std::cmp::Ordering::Equal => 0,
  }
}

/// A participant from a response's `users` map (v1.1 user objects).
pub fn participant(v: &Value, id: &str) -> Option<User> {
  let u = v.at(&format!("users.{id}"));
  let handle = u.str("screen_name")?;
  Some(User {
    id: id.to_owned(),
    name: u.str("name").unwrap_or_else(|| handle.clone()),
    url: Some(user_url(&handle)),
    avatar: u.str("profile_image_url_https"),
    verified: u.bool("is_blue_verified").unwrap_or(false) || u.bool("verified").unwrap_or(false),
    handle: Some(handle),
    ..User::default()
  })
}

/// One timeline entry as a post: messages, and reactions / deletions as events.
pub fn message(entry: &Value, v: &Value, chat_name: Option<&str>) -> Option<Post> {
  let (kind, e) = if entry.at("message").is_object() {
    ("message", entry.at("message"))
  } else if entry.at("reaction_create").is_object() {
    ("reaction", entry.at("reaction_create"))
  } else if entry.at("reaction_delete").is_object() {
    ("unreaction", entry.at("reaction_delete"))
  } else if entry.at("message_delete").is_object() {
    ("delete", entry.at("message_delete"))
  } else {
    return None;
  };
  let conv = e.str("conversation_id")?;
  let data = e.at("message_data");
  let id = data.str("id").or_else(|| e.str("id"))?;
  let sender = data.str("sender_id").or_else(|| e.str("sender_id"));
  let mut post = Post {
    id: message_id(&conv, &id),
    kind: kind.into(),
    text: data
      .str("text")
      .or_else(|| e.str("emoji_reaction"))
      .map(|t| expand(&t, data)),
    author: sender.and_then(|s| participant(v, &s)),
    created_at: e
      .str("time")
      .and_then(|t| t.parse().ok())
      .and_then(from_millis),
    media: attachment(data.at("attachment")),
    raw: Some(entry.clone()),
    ..Post::default()
  };
  let mut put = |key: &str, value: Option<Value>| {
    if let Some(value) = value {
      post.extra.insert(key.into(), value);
    }
  };
  put("chat", Some(conv.clone().into()));
  put("chat_name", chat_name.map(Value::from));
  put(
    "reply_to",
    data
      .str("reply_data.id")
      .map(|r| Value::from(message_id(&conv, &r))),
  );
  put(
    "message",
    e.str("message_id")
      .map(|m| Value::from(message_id(&conv, &m))),
  );
  put(
    "tweet",
    data.str("attachment.tweet.expanded_url").map(Value::from),
  );
  if data.at("edit_count").is_number() {
    put("edits", data.u64("edit_count").map(Value::from));
  }
  Some(post)
}

/// The text with t.co links expanded.
fn expand(text: &str, data: &Value) -> String {
  let mut out = text.to_owned();
  for u in data.list("entities.urls") {
    if let (Some(short), Some(long)) = (u.str("url"), u.str("expanded_url")) {
      out = out.replace(&short, &long);
    }
  }
  out.trim().to_owned()
}
