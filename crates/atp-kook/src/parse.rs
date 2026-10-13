//! KOOK payloads → the shared model.

use atp_core::text::from_millis;
use atp_core::{Collection, Media, MediaKind, Metrics, Post, User, Value, ValueExt, json};

use crate::refs::Chat;

pub fn user(v: &Value) -> User {
  let id = v.str("id").unwrap_or_default();
  let username = v.str("username").unwrap_or_default();
  let handle = match v.str("identify_num") {
    Some(n) => format!("{username}#{n}"),
    None => username.clone(),
  };
  let mut u = User {
    name: v
      .str("nickname")
      .filter(|n| !n.is_empty())
      .unwrap_or(username),
    handle: Some(handle).filter(|h| !h.is_empty()),
    avatar: v.str("avatar").filter(|a| !a.is_empty()),
    verified: v.bool("is_sys") == Some(true),
    id,
    raw: Some(v.clone()),
    ..User::default()
  };
  for key in ["bot", "online"] {
    if let Some(b) = v.bool(key) {
      u.extra.insert(key.into(), json!(b));
    }
  }
  let roles = v.list("roles");
  if !roles.is_empty() {
    u.extra.insert("roles".into(), json!(roles));
  }
  if let Some(t) = v.i64("joined_at").and_then(from_millis) {
    u.created_at = Some(t);
  }
  u
}

pub fn guild(v: &Value) -> Collection {
  let id = v.str("id").unwrap_or_default();
  let mut c = Collection {
    kind: "server".into(),
    name: v.str("name").unwrap_or_default(),
    description: v.str("topic").filter(|t| !t.is_empty()),
    url: v
      .str("open_id")
      .filter(|o| o != "0")
      .map(|o| format!("https://kook.top/{o}")),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(icon) = v.str("icon").filter(|i| !i.is_empty()) {
    c.extra.insert("icon".into(), json!(icon));
  }
  for key in ["default_channel_id", "level", "boost_num", "region"] {
    if let Some(val) = v.str(key).filter(|s| !s.is_empty()) {
      c.extra.insert(key.into(), json!(val));
    }
  }
  c.id = format!("guild:{id}");
  c
}

pub fn channel(v: &Value) -> Collection {
  let kind = if v.bool("is_category") == Some(true) {
    "category"
  } else {
    match v.u64("type") {
      Some(2) => "voice",
      _ => "channel",
    }
  };
  let mut c = Collection {
    id: v.str("id").unwrap_or_default(),
    kind: kind.into(),
    name: format!("#{}", v.str("name").unwrap_or_default()),
    description: v.str("topic").filter(|t| !t.is_empty()),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  for key in ["guild_id", "parent_id"] {
    if let Some(val) = v.str(key).filter(|s| !s.is_empty() && s != "0") {
      c.extra.insert(key.into(), json!(val));
    }
  }
  if let Some(s) = v.u64("slow_mode").filter(|s| *s > 0) {
    c.extra.insert("slowmode_ms".into(), json!(s));
  }
  c
}

/// A direct chat from `/user-chat/list`.
pub fn direct(v: &Value) -> Collection {
  let target = v.at("target_info");
  let id = target.str("id").unwrap_or_default();
  let mut c = Collection {
    kind: "dm".into(),
    name: target.str("username").unwrap_or_else(|| id.clone()),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(n) = v.u64("unread_count") {
    c.extra.insert("unread".into(), json!(n));
  }
  if let Some(t) = v.i64("latest_msg_time").and_then(from_millis) {
    c.extra
      .insert("last_message_at".into(), json!(t.to_string()));
  }
  if let Some(code) = v.str("code") {
    c.extra.insert("chat_code".into(), json!(code));
  }
  c.id = format!("dm:{id}");
  c
}

fn kind(t: u64) -> &'static str {
  match t {
    2 => "image",
    3 => "video",
    4 => "file",
    8 => "audio",
    10 => "card",
    255 => "system",
    _ => "message",
  }
}

fn attachment(a: &Value) -> Option<Media> {
  let url = a.str("url")?;
  let kind = match a.str("type").as_deref() {
    Some("image") => MediaKind::Image,
    Some("video") => MediaKind::Video,
    Some("audio") => MediaKind::Audio,
    _ => MediaKind::File,
  };
  let mut m = Media::new(kind, url);
  m.alt = a.str("name");
  m.size = a.u64("size");
  m.width = a.u64("width").map(|w| w as u32);
  m.height = a.u64("height").map(|h| h as u32);
  m.duration = a.f64("duration");
  Some(m)
}

/// A message of `message/list`, `direct-message/list` or `message/view`.
pub fn message(v: &Value, chat: &Chat, chat_name: Option<&str>) -> Post {
  let id = v.str("id").unwrap_or_default();
  let t = v.u64("type").unwrap_or(1);
  let mut media = Vec::new();
  if let Some(a) = v.at("attachments").as_object().map(|_| v.at("attachments"))
    && let Some(m) = attachment(a)
  {
    media.push(m);
  }
  if t == 2
    && media.is_empty()
    && let Some(url) = v.str("content")
  {
    media.push(Media::image(url));
  }
  let text = match t {
    2 | 3 | 4 | 8 => None,
    _ => v.str("content"),
  };
  let reactions: Vec<Value> = v
    .list("reactions")
    .iter()
    .map(|r| json!({"emoji": r.str("emoji.name").or_else(|| r.str("emoji.id")), "count": r.u64("count"), "me": r.bool("me")}))
    .collect();
  let total: u64 = v
    .list("reactions")
    .iter()
    .filter_map(|r| r.u64("count"))
    .sum();
  let mut p = Post {
    id: format!("{}/{id}", chat.id()),
    kind: kind(t).into(),
    text,
    author: v.at("author").is_object().then(|| user(v.at("author"))),
    created_at: v.i64("create_at").and_then(from_millis),
    updated_at: v.i64("updated_at").filter(|t| *t > 0).and_then(from_millis),
    metrics: Metrics {
      likes: (!reactions.is_empty()).then_some(total),
      ..Metrics::default()
    },
    media,
    raw: Some(v.clone()),
    ..Post::default()
  };
  let e = &mut p.extra;
  e.insert("chat".into(), json!(chat.id()));
  if let Some(name) = chat_name {
    e.insert("chat_name".into(), json!(name));
  }
  e.insert("message_id".into(), json!(id));
  if let Some(q) = v.str("quote.id") {
    e.insert("reply_to".into(), json!(format!("{}/{q}", chat.id())));
  }
  if !reactions.is_empty() {
    e.insert("reactions".into(), json!(reactions));
  }
  if v.bool("mention_all") == Some(true) {
    e.insert("mention_all".into(), json!(true));
  }
  let mentions = v.list("mention");
  if !mentions.is_empty() {
    e.insert("mentions".into(), json!(mentions));
  }
  p
}
