//! VoceChat payloads → the shared model. Channels and direct chats are
//! collections (`group:<gid>`, `user:<uid>`), messages are posts with
//! `<chat>/<mid>` ids. Edits, likes and deletions arrive as reaction
//! messages; within one page they are folded into the messages they touch.

use std::collections::HashMap;

use social_core::text::from_millis;
use social_core::{Collection, Media, MediaKind, Post, User, Value, ValueExt, json};

use crate::refs::Chat;

pub type Users = HashMap<i64, User>;

pub fn user(v: &Value, server: &str) -> User {
  let uid = v.i64("uid").unwrap_or_default();
  let mut u = User {
    id: uid.to_string(),
    name: v.str("name").unwrap_or_default(),
    handle: Some(format!("user:{uid}")),
    url: Some(format!("{server}/#/chat/dm/{uid}")),
    avatar: (v.i64("avatar_updated_at").unwrap_or(0) > 0)
      .then(|| format!("{server}/api/resource/avatar?uid={uid}")),
    raw: Some(v.clone()),
    ..User::default()
  };
  for key in ["is_admin", "is_bot"] {
    if v.bool(key) == Some(true) {
      u.extra.insert(key.into(), json!(true));
    }
  }
  if let Some(e) = v.str("email") {
    u.extra.insert("email".into(), json!(e));
  }
  u
}

pub fn group(v: &Value, server: &str) -> Collection {
  let gid = v.i64("gid").unwrap_or_default();
  let mut c = Collection {
    id: format!("group:{gid}"),
    kind: "channel".into(),
    name: v.str("name").unwrap_or_default(),
    description: v.str("description"),
    url: Some(format!("{server}/#/chat/channel/{gid}")),
    items: None,
    followers: Some(v.list("members").len() as u64).filter(|n| *n > 0),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if v.bool("is_public") == Some(true) {
    c.extra.insert("public".into(), json!(true));
  }
  c
}

pub fn direct(u: &User) -> Collection {
  Collection {
    id: format!("user:{}", u.id),
    kind: "dm".into(),
    name: u.name.clone(),
    url: u.url.clone(),
    owner: Some(u.clone()),
    ..Collection::default()
  }
}

/// The chat a message belongs to, seen from `me` (direct messages from
/// someone else belong to the chat with the sender).
pub fn chat_of(v: &Value, me: Option<i64>) -> Option<Chat> {
  let target = v.at("target");
  if let Some(g) = target.i64("gid") {
    return Some(Chat::Group(g));
  }
  let to = target.i64("uid")?;
  let from = v.i64("from_uid")?;
  Some(Chat::User(if Some(to) == me { from } else { to }))
}

fn file_media(detail: &Value, server: &str) -> Option<Media> {
  if detail.str("content_type").as_deref() != Some("vocechat/file") {
    return None;
  }
  let path = detail.str("content")?;
  let props = detail.at("properties");
  let mime = props.str("content_type").unwrap_or_default();
  let kind = if mime == "image/gif" {
    MediaKind::Gif
  } else if mime.starts_with("image/") {
    MediaKind::Image
  } else if mime.starts_with("video/") {
    MediaKind::Video
  } else if mime.starts_with("audio/") {
    MediaKind::Audio
  } else {
    return None;
  };
  let encoded: String = url::form_urlencoded::byte_serialize(path.as_bytes()).collect();
  let mut m = Media::new(
    kind,
    format!("{server}/api/resource/file?file_path={encoded}&download=true"),
  );
  m.width = props.u64("width").map(|w| w as u32);
  m.height = props.u64("height").map(|h| h as u32);
  m.size = props.u64("size");
  m.alt = props.str("name");
  Some(m)
}

/// One message (normal, reply or reaction) as a post.
pub fn message(v: &Value, chat: &str, chat_name: &str, users: &Users, server: &str) -> Post {
  let mid = v.i64("mid").unwrap_or_default();
  let detail = v.at("detail");
  let kind = detail.str("type").unwrap_or_default();
  let (post_kind, content_holder) = match kind.as_str() {
    "reply" => ("reply", detail),
    "reaction" => match detail.str("detail.type").as_deref() {
      Some("edit") => ("edit", detail.at("detail")),
      Some("like") => ("reaction", detail.at("detail")),
      Some("delete") => ("delete", detail.at("detail")),
      _ => ("reaction", detail.at("detail")),
    },
    _ => ("message", detail),
  };
  let text = match post_kind {
    "reaction" => content_holder.str("action"),
    "delete" => None,
    _ => match content_holder.str("content_type").as_deref() {
      Some("vocechat/file") => content_holder
        .at("properties")
        .str("name")
        .map(|n| format!("[file] {n}")),
      _ => content_holder.str("content"),
    },
  };
  let from = v.i64("from_uid").unwrap_or_default();
  let author = users.get(&from).cloned().unwrap_or_else(|| User {
    id: from.to_string(),
    name: format!("user {from}"),
    handle: Some(format!("user:{from}")),
    ..User::default()
  });
  let mut post = Post {
    id: format!("{chat}/{mid}"),
    kind: post_kind.into(),
    text,
    author: Some(author),
    created_at: v.i64("created_at").and_then(from_millis),
    media: file_media(content_holder, server).into_iter().collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  post.extra.insert("chat".into(), json!(chat));
  if !chat_name.is_empty() {
    post.extra.insert("chat_name".into(), json!(chat_name));
  }
  post.extra.insert("mid".into(), json!(mid));
  if let Some(target) = detail.i64("mid") {
    post.extra.insert("target".into(), json!(target));
  }
  if let Some(ct) = content_holder.str("content_type") {
    post.extra.insert("content_type".into(), json!(ct));
  }
  post
}

/// Fold edits, likes and deletions into the messages of the same page.
pub fn fold(posts: Vec<Post>) -> Vec<Post> {
  let mut out: Vec<Post> = Vec::new();
  let mut changes: Vec<Post> = Vec::new();
  for p in posts {
    if matches!(p.kind.as_str(), "edit" | "reaction" | "delete") {
      changes.push(p);
    } else {
      out.push(p);
    }
  }
  for c in changes {
    let target = c.extra.get("target").and_then(Value::as_i64);
    let Some(msg) = out
      .iter_mut()
      .find(|p| p.extra.get("mid").and_then(Value::as_i64) == target)
    else {
      continue;
    };
    match c.kind.as_str() {
      "edit" => {
        msg.text = c.text;
        msg.extra.insert("edited".into(), json!(true));
      }
      "delete" => {
        msg.kind = "deleted".into();
        msg.text = None;
      }
      _ => {
        *msg.metrics.likes.get_or_insert(0) += 1;
        let list = msg
          .extra
          .entry("reactions".to_owned())
          .or_insert_with(|| json!([]));
        if let Some(a) = list.as_array_mut() {
          a.push(json!({
            "reaction": c.text,
            "user": c.author.map(|u| u.name),
          }));
        }
      }
    }
  }
  out
}
