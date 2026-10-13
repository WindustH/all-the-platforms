//! Upstream payloads → core model: `WeixinMessage` → `Post`, a recorded
//! contact → a chat `Collection`.
//!
//! A message holds an `item_list` of typed items (1 text, 2 image, 3 voice,
//! 4 file, 5 video), optionally quoting another message (`ref_msg`). Media
//! items point to encrypted CDN files; their `Media.url` is
//! `wechat-media:CHAT/MESSAGE/INDEX`, which `download` resolves.

use social_core::chat::message_id;
use social_core::text::from_millis;
use social_core::{Collection, Media, MediaKind, Post, User, Value, ValueExt, json};

use crate::archive::Contact;

/// `message_type` of messages from users; the bot's own are 2.
const FROM_USER: i64 = 1;

pub const SCHEME: &str = "wechat-media:";

/// The conversation a message belongs to: the group, else the other person.
pub fn chat_id(m: &Value) -> String {
  if let Some(g) = m.str("group_id") {
    return g;
  }
  let field = if m.i64("message_type") == Some(FROM_USER) {
    "from_user_id"
  } else {
    "to_user_id"
  };
  m.str(field).unwrap_or_default()
}

pub fn label(item: &Value) -> Option<String> {
  match item.i64("type").unwrap_or(0) {
    1 => item.str("text_item.text"),
    2 => Some("[图片]".into()),
    3 => Some(
      item
        .str("voice_item.text")
        .unwrap_or_else(|| "[语音]".into()),
    ),
    4 => Some(format!(
      "[文件] {}",
      item.str("file_item.file_name").unwrap_or_default()
    )),
    5 => Some("[视频]".into()),
    _ => None,
  }
}

/// The message as one line of text.
pub fn summary(m: &Value) -> String {
  m.list("item_list")
    .iter()
    .filter_map(label)
    .collect::<Vec<_>>()
    .join(" ")
}

pub fn post(m: &Value) -> Post {
  let chat = chat_id(m);
  let message = m
    .str("message_id")
    .or_else(|| m.str("client_id"))
    .unwrap_or_default();
  let id = message_id(&chat, &message);
  let from_user = m.i64("message_type") == Some(FROM_USER);
  let author = if from_user {
    m.str("from_user_id")
  } else {
    Some("bot".into())
  };
  let items = m.list("item_list");
  let text = items
    .iter()
    .filter(|i| i.i64("type") != Some(2) && i.i64("type") != Some(5))
    .filter_map(label)
    .collect::<Vec<_>>()
    .join("\n");
  let media = items
    .iter()
    .enumerate()
    .filter_map(|(n, item)| media(item, &format!("{SCHEME}{id}/{n}")))
    .collect();
  let quoted = items
    .iter()
    .find_map(|i| i.get("ref_msg"))
    .map(|r| Box::new(quote(r)));
  let mut post = Post {
    id,
    kind: "message".into(),
    text: Some(text).filter(|t| !t.is_empty()),
    author: author.map(|a| User {
      id: a.clone(),
      name: a,
      ..User::default()
    }),
    created_at: m.i64("create_time_ms").and_then(from_millis),
    updated_at: m.i64("update_time_ms").and_then(from_millis),
    media,
    quoted,
    raw: Some(m.clone()),
    ..Post::default()
  };
  post.extra.insert("chat".into(), chat.into());
  post.extra.insert("message_id".into(), message.into());
  post.extra.insert(
    "direction".into(),
    if from_user { "in" } else { "out" }.into(),
  );
  post
}

/// A quoted message: the summary WeChat sends, and the quoted item when present.
fn quote(r: &Value) -> Post {
  let item = r.at("message_item");
  let text = label(item).or_else(|| r.str("title"));
  Post {
    id: r
      .str("svr_id")
      .or_else(|| item.str("msg_id"))
      .unwrap_or_default(),
    kind: "quote".into(),
    text,
    author: item.str("from_user_id").map(|a| User {
      id: a.clone(),
      name: a,
      ..User::default()
    }),
    created_at: item.i64("create_time_ms").and_then(from_millis),
    ..Post::default()
  }
}

fn media(item: &Value, url: &str) -> Option<Media> {
  let (kind, field) = match item.i64("type")? {
    2 => (MediaKind::Image, "image_item"),
    3 => (MediaKind::Audio, "voice_item"),
    4 => (MediaKind::File, "file_item"),
    5 => (MediaKind::Video, "video_item"),
    _ => return None,
  };
  let v = item.at(field);
  let m = v.at("media");
  if m.str("encrypt_query_param").is_none() && m.str("full_url").is_none() {
    return None;
  }
  let mut media = Media::new(kind, url);
  media.alt = v.str("file_name");
  media.size = v
    .u64("len")
    .or_else(|| v.u64("hd_size"))
    .or_else(|| v.u64("mid_size"))
    .or_else(|| v.u64("video_size"));
  // Voice length is in milliseconds.
  media.duration = v.f64("playtime").map(|ms| ms / 1000.0);
  Some(media)
}

/// The text item of an outgoing message, as the record keeps it.
pub fn text_item(text: &str) -> Value {
  json!({ "type": 1, "text_item": { "text": text } })
}

/// A conversation on record, as a chat.
pub fn chat(contact: Contact) -> Collection {
  let mut chat = Collection {
    id: contact.id.clone(),
    kind: "dm".into(),
    name: contact.id,
    description: Some(contact.last_text).filter(|t| !t.is_empty()),
    items: Some(contact.messages),
    ..Collection::default()
  };
  if let Some(at) = from_millis(contact.last_at) {
    chat.extra.insert("last_at".into(), at.to_string().into());
  }
  chat
    .extra
    .insert("can_reply".into(), contact.context_token.is_some().into());
  chat
}
