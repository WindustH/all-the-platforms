//! Bot API payloads → the shared model: users, chats as users, messages.

use social_core::text::from_secs;
use social_core::{Media, MediaKind, Post, User, Value, ValueExt, json};

use crate::refs;

/// A Bot API `User`.
pub(super) fn user(u: &Value) -> User {
  let name = [u.str("first_name"), u.str("last_name")]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
  let handle = u.str("username");
  let mut out = User {
    id: u.str("id").unwrap_or_default(),
    name,
    url: handle.as_ref().map(|h| format!("https://t.me/{h}")),
    handle,
    ..User::default()
  };
  let kind = if u.bool("is_bot") == Some(true) {
    "bot"
  } else {
    "user"
  };
  out.extra.insert("kind".into(), kind.into());
  out
}

/// A Bot API `Chat` / `ChatFullInfo`, as a user.
pub(super) fn chat_user(chat: &Value) -> User {
  let handle = chat.str("username");
  let name = chat.str("title").unwrap_or_else(|| user(chat).name);
  let mut out = User {
    id: chat.str("id").unwrap_or_default(),
    name,
    url: handle.as_ref().map(|h| format!("https://t.me/{h}")),
    handle,
    bio: chat.first_str(&["bio", "description"]),
    ..User::default()
  };
  out
    .extra
    .insert("kind".into(), chat.str("type").unwrap_or_default().into());
  if let Some(linked) = chat.i64("linked_chat_id") {
    out.extra.insert("linked_chat".into(), linked.into());
  }
  out
}

/// A Bot API `Message`; files point at `tg-bot-file:` for `download`.
pub fn post(message: &Value) -> Post {
  let chat = message.at("chat");
  let chat_id = chat.i64("id").unwrap_or_default();
  let username = chat.str("username");
  let id = message.i64("message_id").unwrap_or_default() as i32;
  let author = if message.get("from").is_some() {
    Some(user(message.at("from")))
  } else if message.get("sender_chat").is_some() {
    Some(chat_user(message.at("sender_chat")))
  } else {
    None
  };
  let mut post = Post {
    id: refs::message_id(username.as_deref(), chat_id, id),
    kind: "message".into(),
    text: message.first_str(&["text", "caption"]),
    url: refs::message_url(username.as_deref(), chat_id, id),
    author,
    created_at: message.i64("date").and_then(from_secs),
    updated_at: message.i64("edit_date").and_then(from_secs),
    ..Post::default()
  };
  post.media = files(message);
  let extra = &mut post.extra;
  extra.insert("chat".into(), chat_id.into());
  let chat_name = chat.first_str(&["title", "first_name"]).unwrap_or_default();
  extra.insert("chat_name".into(), chat_name.into());
  extra.insert("message_id".into(), id.into());
  if let Some(r) = message.i64("reply_to_message.message_id") {
    extra.insert(
      "reply_to".into(),
      refs::message_id(username.as_deref(), chat_id, r as i32).into(),
    );
  }
  if let Some(g) = message.str("media_group_id") {
    extra.insert("album".into(), g.into());
  }
  if let Some(origin) = message.get("forward_origin") {
    extra.insert("forwarded_from".into(), origin.clone());
  }
  let counts: Vec<Value> = message
    .list("reactions")
    .iter()
    .map(
      |r| json!({"emoji": r.str("type.emoji").unwrap_or_default(), "count": r.u64("total_count")}),
    )
    .collect();
  if !counts.is_empty() {
    post.metrics.likes = Some(counts.iter().filter_map(|c| c["count"].as_u64()).sum());
    post.extra.insert("reactions".into(), Value::Array(counts));
  }
  post.raw = Some(message.clone());
  post
}

fn files(m: &Value) -> Vec<Media> {
  let file = |kind: MediaKind, v: &Value| {
    let mut media = Media::new(kind, format!("tg-bot-file:{}", v.str("file_id")?));
    media.size = v.u64("file_size");
    media.width = v.u64("width").map(|n| n as u32);
    media.height = v.u64("height").map(|n| n as u32);
    media.duration = v.f64("duration");
    media.alt = v.str("file_name");
    Some(media)
  };
  let mut out = Vec::new();
  // The largest size of a photo comes last.
  if let Some(photo) = m.list("photo").last() {
    out.extend(file(MediaKind::Image, photo));
  }
  for (key, kind) in [
    ("video", MediaKind::Video),
    ("animation", MediaKind::Gif),
    ("audio", MediaKind::Audio),
    ("voice", MediaKind::Audio),
    ("video_note", MediaKind::Video),
    ("sticker", MediaKind::Image),
    ("document", MediaKind::File),
  ] {
    if let Some(v) = m.get(key) {
      // An animation also comes as a document.
      if key == "document" && m.get("animation").is_some() {
        continue;
      }
      out.extend(file(kind, v));
    }
  }
  out
}
