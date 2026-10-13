//! QQ bot payloads → the shared model.

use atp_core::text::parse_time;
use atp_core::{Collection, Media, MediaKind, Post, User, Value, ValueExt, json};

use crate::refs::Chat;

pub fn user(v: &Value) -> User {
  let id = v
    .first_str(&["id", "user_openid", "member_openid", "union_openid"])
    .unwrap_or_default();
  let mut u = User {
    name: v.str("username").unwrap_or_else(|| id.clone()),
    avatar: v.str("avatar").filter(|a| !a.is_empty()),
    id,
    raw: Some(v.clone()),
    ..User::default()
  };
  if v.bool("bot") == Some(true) {
    u.extra.insert("bot".into(), json!(true));
  }
  for key in ["union_openid", "member_openid", "user_openid"] {
    if let Some(val) = v.str(key) {
      u.extra.insert(key.into(), json!(val));
    }
  }
  u
}

/// A guild member: the user with the guild's nickname and roles.
pub fn member(v: &Value) -> User {
  let mut u = user(v.at("user"));
  if let Some(nick) = v.str("nick").filter(|n| !n.is_empty()) {
    u.extra.insert("username".into(), json!(u.name));
    u.name = nick;
  }
  let roles = v.list("roles");
  if !roles.is_empty() {
    u.extra.insert("roles".into(), json!(roles));
  }
  if let Some(t) = v.str("joined_at") {
    u.extra.insert("joined_at".into(), json!(t));
  }
  u.raw = Some(v.clone());
  u
}

pub fn guild(v: &Value) -> Collection {
  let id = v.str("id").unwrap_or_default();
  let mut c = Collection {
    kind: "server".into(),
    name: v.str("name").unwrap_or_default(),
    description: v.str("description").filter(|d| !d.is_empty()),
    followers: v.u64("member_count"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  for key in ["owner_id", "max_members", "joined_at", "icon"] {
    if let Some(val) = v.str(key).filter(|s| !s.is_empty()) {
      c.extra.insert(key.into(), json!(val));
    }
  }
  if v.bool("owner") == Some(true) {
    c.extra.insert("owner".into(), json!(true));
  }
  c.id = format!("guild:{id}");
  c
}

pub fn channel(v: &Value) -> Collection {
  let kind = match v.u64("type") {
    Some(2) => "voice",
    Some(4) => "category",
    Some(10005) => "live",
    Some(10006) => "app",
    Some(10007) => "forum",
    _ => "channel",
  };
  let mut c = Collection {
    id: v.str("id").unwrap_or_default(),
    kind: kind.into(),
    name: format!("#{}", v.str("name").unwrap_or_default()),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  for key in ["guild_id", "parent_id"] {
    if let Some(val) = v.str(key).filter(|s| !s.is_empty() && s != "0") {
      c.extra.insert(key.into(), json!(val));
    }
  }
  if let Some(p) = v.u64("private_type").filter(|p| *p > 0) {
    c.extra.insert("private".into(), json!(p));
  }
  c
}

fn attachment(a: &Value) -> Option<Media> {
  let url = a.str("url")?;
  let url = if url.starts_with("http") {
    url
  } else {
    format!("https://{url}")
  };
  let ct = a.str("content_type").unwrap_or_default();
  let kind = if ct == "image/gif" {
    MediaKind::Gif
  } else if ct.starts_with("image") {
    MediaKind::Image
  } else if ct.starts_with("video") {
    MediaKind::Video
  } else if ct.starts_with("audio") || ct == "voice" {
    MediaKind::Audio
  } else {
    MediaKind::File
  };
  let mut m = Media::new(kind, url);
  m.alt = a.str("filename");
  m.size = a.u64("size");
  m.width = a.u64("width").map(|w| w as u32);
  m.height = a.u64("height").map(|h| h as u32);
  Some(m)
}

/// `<@!id>` → `@name` with the message's mentions.
fn readable(content: &str, mentions: &[Value]) -> String {
  let mut text = content.to_owned();
  for m in mentions {
    if let (Some(id), Some(name)) = (m.str("id"), m.str("username")) {
      for token in [format!("<@!{id}>"), format!("<@{id}>")] {
        text = text.replace(&token, &format!("@{name}"));
      }
    }
  }
  text.trim().to_owned()
}

/// A message of any scene: channel, 频道私信, group or C2C.
pub fn message(v: &Value, chat: &Chat) -> Post {
  let id = v.str("id").unwrap_or_default();
  let content = v.str("content").unwrap_or_default();
  let mut author = v.at("author").is_object().then(|| user(v.at("author")));
  if let (Some(a), Some(nick)) = (
    author.as_mut(),
    v.str("member.nick").filter(|n| !n.is_empty()),
  ) {
    a.name = nick;
  }
  let mut p = Post {
    id: format!("{}/{id}", chat.id()),
    kind: "message".into(),
    text: Some(readable(&content, v.list("mentions"))).filter(|t| !t.is_empty()),
    author,
    created_at: v.str("timestamp").and_then(|t| parse_time(&t)),
    media: v
      .list("attachments")
      .iter()
      .filter_map(attachment)
      .collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let e = &mut p.extra;
  e.insert("chat".into(), json!(chat.id()));
  e.insert("message_id".into(), json!(id));
  if let Some(g) = v.str("guild_id") {
    e.insert("guild".into(), json!(g));
  }
  if let Some(r) = v.str("message_reference.message_id") {
    e.insert("reply_to".into(), json!(format!("{}/{r}", chat.id())));
  }
  if let Some(seq) = v.str("seq_in_channel") {
    e.insert("seq".into(), json!(seq));
  }
  p
}
