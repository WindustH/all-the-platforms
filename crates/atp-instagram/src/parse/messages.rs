//! Direct threads as chats and their items as messages (`THREAD/ITEM`).

use std::collections::BTreeMap;

use atp_core::text::from_secs;
use atp_core::{Collection, Media, MediaKind, Post, User, Value, ValueExt, chat};

use super::{post, user};

/// Paths of an item's shared or sent media.
const MEDIA_PATHS: [&str; 4] = ["media", "visual_media.media", "clip.clip", "media_share"];

pub fn chat(thread: &Value) -> Collection {
  let users: Vec<String> = thread
    .list("users")
    .iter()
    .filter_map(|u| u.str("username"))
    .collect();
  Collection {
    id: thread.str("thread_id").unwrap_or_default(),
    kind: if thread.bool("is_group") == Some(true) {
      "group".into()
    } else {
      "dm".into()
    },
    name: thread
      .str("thread_title")
      .filter(|n| !n.is_empty())
      .unwrap_or_else(|| users.join(", ")),
    description: thread.list("items").first().and_then(|i| i.str("text")),
    extra: [
      ("users".to_owned(), Value::from(users)),
      (
        "last_activity_at".to_owned(),
        thread.at("last_activity_at").clone(),
      ),
      (
        "unread".to_owned(),
        Value::from(thread.i64("read_state") == Some(1)),
      ),
    ]
    .into(),
    raw: Some(thread.clone()),
    ..Collection::default()
  }
}

/// A thread's members by id, the inviter (the logged-in account) included.
pub fn members(thread: &Value) -> BTreeMap<String, User> {
  let mut out: BTreeMap<String, User> = thread
    .list("users")
    .iter()
    .map(|u| {
      let u = user(u);
      (u.id.clone(), u)
    })
    .collect();
  if thread.at("inviter").is_object() {
    let me = user(thread.at("inviter"));
    out.entry(me.id.clone()).or_insert(me);
  }
  out
}

/// An item of thread `chat`, its author looked up among `members`.
pub fn message(chat: &str, item: &Value, members: &BTreeMap<String, User>) -> Post {
  let kind = item.str("item_type").unwrap_or_else(|| "text".into());
  let sender = item.str("user_id").unwrap_or_default();
  let mut media = Vec::new();
  for path in MEDIA_PATHS {
    let shared = item.at(path);
    if shared.is_object() {
      media.extend(post(shared).media);
    }
  }
  if let Some(url) = item.str("voice_media.media.audio.audio_src") {
    media.push(Media::new(MediaKind::Audio, url));
  }
  Post {
    id: chat::message_id(chat, &item.str("item_id").unwrap_or_default()),
    kind: if kind == "text" {
      "message".into()
    } else {
      kind
    },
    text: item
      .str("text")
      .or_else(|| item.str("link.text"))
      .or_else(|| item.str("reel_share.text")),
    author: Some(members.get(&sender).cloned().unwrap_or(User {
      id: sender,
      ..User::default()
    })),
    // Microseconds.
    created_at: item
      .i64("timestamp")
      .map(|t| t / 1_000_000)
      .and_then(from_secs),
    media,
    extra: [("chat".to_owned(), Value::from(chat))].into(),
    raw: Some(item.clone()),
    ..Post::default()
  }
}
