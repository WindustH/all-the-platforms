//! Misskey payloads → the shared model: notes are posts (renotes quote the
//! renoted note), users are users, chat rooms and direct chats are
//! collections, chat messages are posts with `user:<id>/<message>` or
//! `room:<id>/<message>` ids.

use std::collections::BTreeMap;

use social_core::text::parse_time;
use social_core::{
  Collection, Comment, Media, MediaKind, Metrics, Notification, Post, User, UserStats, Value,
  ValueExt, json,
};

pub fn user(v: &Value, instance: &str) -> User {
  let username = v.str("username").unwrap_or_default();
  let host = v.str("host");
  let handle = match &host {
    Some(h) => format!("@{username}@{h}"),
    None => format!("@{username}"),
  };
  let mut u = User {
    id: v.str("id").unwrap_or_default(),
    name: v.str("name").unwrap_or_else(|| username.clone()),
    url: (!username.is_empty()).then(|| format!("https://{instance}/{handle}")),
    handle: Some(handle),
    avatar: v.str("avatarUrl"),
    bio: v.str("description"),
    location: v.str("location"),
    created_at: v.str("createdAt").and_then(|t| parse_time(&t)),
    followed: v.bool("isFollowing"),
    stats: UserStats {
      followers: v.u64("followersCount"),
      following: v.u64("followingCount"),
      posts: v.u64("notesCount"),
      ..UserStats::default()
    },
    raw: Some(v.clone()),
    ..User::default()
  };
  for key in ["isBot", "isCat", "isLocked"] {
    if v.bool(key) == Some(true) {
      u.extra.insert(key.into(), json!(true));
    }
  }
  if let Some(h) = host {
    u.extra.insert("host".into(), json!(h));
  }
  u
}

pub fn note_url(instance: &str, id: &str) -> String {
  format!("https://{instance}/notes/{id}")
}

fn files(v: &Value) -> Vec<Media> {
  v.list("files")
    .iter()
    .filter_map(|f| {
      let url = f.str("url")?;
      let mime = f.str("type").unwrap_or_default();
      let kind = if mime == "image/gif" {
        MediaKind::Gif
      } else if mime.starts_with("video/") {
        MediaKind::Video
      } else if mime.starts_with("audio/") {
        MediaKind::Audio
      } else if mime.starts_with("image/") {
        MediaKind::Image
      } else {
        return None;
      };
      let mut m = Media::new(kind, url);
      m.width = f.u64("properties.width").map(|w| w as u32);
      m.height = f.u64("properties.height").map(|h| h as u32);
      m.size = f.u64("size");
      m.alt = f.str("comment");
      Some(m)
    })
    .collect()
}

/// Files that are not images, video or audio, as links.
fn other_files(v: &Value) -> Vec<Value> {
  v.list("files")
    .iter()
    .filter(|f| {
      let t = f.str("type").unwrap_or_default();
      !(t.starts_with("image/") || t.starts_with("video/") || t.starts_with("audio/"))
    })
    .map(|f| json!({"name": f.str("name"), "url": f.str("url"), "type": f.str("type")}))
    .collect()
}

pub fn note(v: &Value, instance: &str) -> Post {
  let id = v.str("id").unwrap_or_default();
  let text = v.str("text");
  let renote = v.at("renote");
  let is_pure_renote = text.is_none() && !renote.is_null() && v.list("files").is_empty();
  let kind = if is_pure_renote {
    "renote"
  } else if v.str("replyId").is_some() {
    "reply"
  } else {
    "note"
  };
  let mut post = Post {
    url: Some(note_url(instance, &id)),
    id,
    kind: kind.into(),
    text,
    author: Some(user(v.at("user"), instance)),
    created_at: v.str("createdAt").and_then(|t| parse_time(&t)),
    metrics: Metrics {
      likes: v.u64("reactionCount"),
      comments: v.u64("repliesCount"),
      shares: v.u64("renoteCount"),
      other: v
        .u64("clippedCount")
        .filter(|n| *n > 0)
        .map(|n| BTreeMap::from([("clips".to_owned(), n)]))
        .unwrap_or_default(),
      ..Metrics::default()
    },
    media: files(v),
    tags: v
      .list("tags")
      .iter()
      .filter_map(|t| t.as_str().map(str::to_owned))
      .collect(),
    quoted: (!renote.is_null()).then(|| Box::new(note(renote, instance))),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let mut put = |key: &str, value: Value| {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  };
  put("cw", json!(v.str("cw")));
  put("visibility", json!(v.str("visibility")));
  put("reply_id", json!(v.str("replyId")));
  put("renote_id", json!(v.str("renoteId")));
  put("uri", json!(v.str("uri").or_else(|| v.str("url"))));
  put("channel", json!(v.str("channel.name")));
  if v.at("reactions").as_object().is_some_and(|m| !m.is_empty()) {
    put("reactions", v.at("reactions").clone());
  }
  if let Some(r) = v.str("myReaction") {
    put("my_reaction", json!(r));
  }
  let others = other_files(v);
  if !others.is_empty() {
    put("files", json!(others));
  }
  post
}

/// A reply note as a comment, with its own replies when the API sent them.
pub fn comment(v: &Value, instance: &str) -> Comment {
  let p = note(v, instance);
  Comment {
    id: p.id,
    author: p.author,
    text: p.text.unwrap_or_default(),
    created_at: p.created_at,
    likes: p.metrics.likes,
    reply_count: p.metrics.comments,
    reply_to: v.str("reply.user.username").map(|u| format!("@{u}")),
    extra: p.extra,
    raw: p.raw,
    ..Comment::default()
  }
}

pub fn notification(v: &Value, instance: &str) -> Notification {
  let kind = v.str("type").unwrap_or_default();
  let note = v.at("note");
  let text = match kind.as_str() {
    "reaction" => v.str("reaction").unwrap_or_default(),
    "follow" => "followed you".into(),
    "receiveFollowRequest" => "asked to follow you".into(),
    "followRequestAccepted" => "accepted your follow request".into(),
    "achievementEarned" => v.str("achievement").unwrap_or_default(),
    "app" => v.str("body").unwrap_or_default(),
    _ => note.str("text").unwrap_or_default(),
  };
  let shared = match kind.as_str() {
    "reaction" => "like",
    "renote" => "repost",
    "receiveFollowRequest" => "follow_request",
    other => other,
  };
  Notification {
    id: v.str("id").unwrap_or_default(),
    kind: shared.to_owned(),
    text,
    actor: (!v.at("user").is_null()).then(|| user(v.at("user"), instance)),
    target: note
      .str("text")
      .or_else(|| note.str("renote.text"))
      .filter(|_| kind == "reaction" || kind == "renote"),
    url: note.str("id").map(|id| note_url(instance, &id)),
    created_at: v.str("createdAt").and_then(|t| parse_time(&t)),
    unread: v.bool("isRead").map(|r| !r),
    raw: Some(v.clone()),
  }
}

pub fn hashtag(v: &Value) -> Collection {
  let tag = v.str("tag").or_else(|| v.as_str().map(str::to_owned));
  let tag = tag.unwrap_or_default();
  Collection {
    id: tag.clone(),
    kind: "hashtag".into(),
    name: format!("#{tag}"),
    followers: v.u64("usersCount"),
    raw: Some(v.clone()),
    ..Collection::default()
  }
}

/// A chat room as a collection (`room:<id>`).
pub fn room(v: &Value, instance: &str) -> Collection {
  let id = v.str("id").unwrap_or_default();
  Collection {
    id: format!("room:{id}"),
    kind: "room".into(),
    name: v.str("name").unwrap_or_default(),
    description: v.str("description"),
    url: Some(format!("https://{instance}/chat/room/{id}")),
    owner: (!v.at("owner").is_null()).then(|| user(v.at("owner"), instance)),
    raw: Some(v.clone()),
    ..Collection::default()
  }
}

/// A direct chat with `other` as a collection (`user:<id>`).
pub fn direct(other: &Value, instance: &str) -> Collection {
  let u = user(other, instance);
  Collection {
    id: format!("user:{}", u.id),
    kind: "dm".into(),
    name: u.name.clone(),
    description: u.handle.clone(),
    url: Some(format!("https://{instance}/chat/user/{}", u.id)),
    owner: Some(u),
    raw: Some(other.clone()),
    ..Collection::default()
  }
}

/// A chat message; `chat` is the `user:<id>` / `room:<id>` it belongs to.
pub fn chat_message(v: &Value, chat: &str, chat_name: &str, instance: &str) -> Post {
  let mid = v.str("id").unwrap_or_default();
  let file = v.at("file");
  let media = if file.is_null() {
    Vec::new()
  } else {
    files(&json!({"files": [file]}))
  };
  let mut post = Post {
    id: format!("{chat}/{mid}"),
    kind: "message".into(),
    text: v.str("text"),
    author: (!v.at("fromUser").is_null()).then(|| user(v.at("fromUser"), instance)),
    created_at: v.str("createdAt").and_then(|t| parse_time(&t)),
    media,
    raw: Some(v.clone()),
    ..Post::default()
  };
  post.extra.insert("chat".into(), json!(chat));
  if !chat_name.is_empty() {
    post.extra.insert("chat_name".into(), json!(chat_name));
  }
  post.extra.insert("message_id".into(), json!(mid));
  let reactions = v.list("reactions");
  if !reactions.is_empty() {
    post.metrics.likes = Some(reactions.len() as u64);
    post.extra.insert(
      "reactions".into(),
      json!(
        reactions
          .iter()
          .map(|r| json!({"reaction": r.str("reaction"), "user": r.str("user.username")}))
          .collect::<Vec<_>>()
      ),
    );
  }
  post
}
