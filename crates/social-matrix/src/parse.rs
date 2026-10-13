//! Matrix events → the shared model. A message is a post with the id
//! `ROOM/$event` and a matrix.to permalink; rooms are collections; edits,
//! reactions, redactions and encrypted events keep their kind.

use std::collections::HashMap;

use social_core::chat;
use social_core::text::{from_millis, html_to_text};
use social_core::{Collection, Media, MediaKind, Post, User, Value, ValueExt, json};

use crate::refs::permalink;

static NULL: Value = Value::Null;

/// A key with dots in it (`m.relates_to`), which `ValueExt` paths would split.
pub fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
  v.get(key).unwrap_or(&NULL)
}

/// Display names and avatars of senders, from member events.
#[derive(Default)]
pub struct Members(pub HashMap<String, (Option<String>, Option<String>)>);

impl Members {
  pub fn absorb(&mut self, events: &[Value]) {
    for e in events {
      if e.str("type").as_deref() != Some("m.room.member") {
        continue;
      }
      if let Some(id) = e.str("state_key") {
        self.0.insert(
          id,
          (e.str("content.displayname"), e.str("content.avatar_url")),
        );
      }
    }
  }

  pub fn user(&self, id: &str) -> User {
    let (name, avatar) = self.0.get(id).cloned().unwrap_or_default();
    user(id, name, avatar)
  }
}

pub fn user(id: &str, name: Option<String>, avatar: Option<String>) -> User {
  let local = id
    .trim_start_matches('@')
    .split(':')
    .next()
    .unwrap_or(id)
    .to_owned();
  let mut u = User {
    id: id.to_owned(),
    name: name.filter(|n| !n.is_empty()).unwrap_or(local),
    handle: Some(id.to_owned()),
    url: Some(permalink(id, None)),
    ..User::default()
  };
  if let Some(a) = avatar {
    u.extra.insert("avatar_mxc".into(), json!(a));
  }
  u
}

/// One timeline event of `room` as a post, or `None` for state and noise.
pub fn event(room: &str, room_name: &str, e: &Value, members: &Members) -> Option<Post> {
  let kind = e.str("type")?;
  let id = e.str("event_id")?;
  let content = e.at("content");
  let relates = field(content, "m.relates_to");
  let (post_kind, text) = match kind.as_str() {
    "m.room.message" | "m.sticker" => {
      if relates.str("rel_type").as_deref() == Some("m.replace") {
        let new = field(content, "m.new_content");
        ("edit", text_of(if new.is_null() { content } else { new }))
      } else {
        let k = match content.str("msgtype").as_deref() {
          Some("m.notice") => "notice",
          Some("m.emote") => "emote",
          _ if kind == "m.sticker" => "sticker",
          _ => "message",
        };
        (k, text_of(content))
      }
    }
    "m.reaction" => ("reaction", relates.str("key")),
    "m.room.redaction" => ("delete", content.str("reason")),
    "m.room.encrypted" => ("encrypted", Some("(encrypted)".to_owned())),
    _ => return None,
  };
  let sender = e.str("sender").unwrap_or_default();
  let mut post = Post {
    id: chat::message_id(room, &id),
    kind: post_kind.into(),
    text,
    url: Some(permalink(room, Some(&id))),
    author: Some(members.user(&sender)),
    created_at: e.i64("origin_server_ts").and_then(from_millis),
    raw: Some(e.clone()),
    ..Post::default()
  };
  if let Some(m) = media(content) {
    post.media.push(m);
  }
  post.extra.insert("chat".into(), json!(room));
  if !room_name.is_empty() {
    post.extra.insert("chat_name".into(), json!(room_name));
  }
  post.extra.insert("event_id".into(), json!(id));
  if let Some(target) = relates
    .str("event_id")
    .or_else(|| e.str("redacts"))
    .or_else(|| content.str("redacts"))
  {
    post.extra.insert("target".into(), json!(target));
  }
  if let Some(r) = field(relates, "m.in_reply_to").str("event_id") {
    post.extra.insert("reply_to".into(), json!(r));
  }
  if let Some(url) = content.str("url") {
    post.extra.insert("mxc".into(), json!(url));
  }
  Some(post)
}

fn text_of(content: &Value) -> Option<String> {
  let body = content.str("body");
  let html = content
    .str("formatted_body")
    .filter(|_| content.str("format").as_deref() == Some("org.matrix.custom.html"));
  match (html, body) {
    // Replies carry the quoted message in `<mx-reply>`; the body after it is ours.
    (Some(h), _) => {
      let h = match h.find("</mx-reply>") {
        Some(at) => h[at + 11..].to_owned(),
        None => h,
      };
      Some(html_to_text(&h)).filter(|t| !t.is_empty())
    }
    (None, b) => b,
  }
}

/// The file of an `m.image` / `m.video` / `m.audio` / `m.file`, as an
/// `mxc://` URL (turned into a download URL by the caller).
fn media(content: &Value) -> Option<Media> {
  let url = content.str("url")?;
  let kind = match content.str("msgtype").as_deref() {
    Some("m.image") => MediaKind::Image,
    Some("m.video") => MediaKind::Video,
    Some("m.audio") => MediaKind::Audio,
    _ => match content.str("info.mimetype") {
      Some(m) if m.starts_with("image/") => MediaKind::Image,
      Some(m) if m.starts_with("video/") => MediaKind::Video,
      Some(m) if m.starts_with("audio/") => MediaKind::Audio,
      _ => return None,
    },
  };
  let mut m = Media::new(kind, url);
  m.width = content.u64("info.w").map(|w| w as u32);
  m.height = content.u64("info.h").map(|h| h as u32);
  m.size = content.u64("info.size");
  m.duration = content.f64("info.duration").map(|d| d / 1000.0);
  m.alt = content.str("body");
  Some(m)
}

/// A public-directory or hierarchy entry as a collection.
pub fn directory_room(v: &Value) -> Collection {
  let id = v.str("room_id").unwrap_or_default();
  let alias = v.str("canonical_alias");
  let mut c = Collection {
    url: Some(permalink(alias.as_deref().unwrap_or(&id), None)),
    name: v
      .str("name")
      .or_else(|| alias.clone())
      .unwrap_or_else(|| id.clone()),
    kind: if v.str("room_type").as_deref() == Some("m.space") {
      "space".into()
    } else {
      "room".into()
    },
    description: v.str("topic"),
    followers: v.u64("num_joined_members"),
    id,
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(a) = alias {
    c.extra.insert("alias".into(), json!(a));
  }
  for key in ["world_readable", "guest_can_join"] {
    if v.bool(key) == Some(true) {
      c.extra.insert(key.into(), json!(true));
    }
  }
  if let Some(rule) = v.str("join_rule") {
    c.extra.insert("join_rule".into(), json!(rule));
  }
  c
}
