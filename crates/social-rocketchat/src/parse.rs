//! Rocket.Chat payloads → the shared model: messages with links
//! (`SERVER/channel/NAME?msg=ID`), rooms as collections, users, and
//! attachments as media fetched with the auth headers.

use std::collections::HashMap;

use jiff::Timestamp;
use social_core::text::{from_millis, parse_time};
use social_core::{
  Collection, Comment, Media, MediaKind, Metrics, Post, User, Value, ValueExt, json,
};

use crate::api::Creds;
use crate::rooms::RoomInfo;

pub struct View<'a> {
  pub creds: &'a Creds,
  pub rooms: &'a HashMap<String, RoomInfo>,
}

impl View<'_> {
  fn absolute(&self, url: &str) -> String {
    if url.starts_with("http") {
      url.to_owned()
    } else {
      format!("{}{url}", self.creds.server)
    }
  }
}

/// REST sends ISO strings, the realtime API `{"$date": ms}`.
pub fn time(v: &Value) -> Option<Timestamp> {
  match v {
    Value::String(s) => parse_time(s),
    _ => v.i64("$date").and_then(from_millis),
  }
}

pub fn user(v: &Value, server: &str) -> User {
  let username = v.str("username");
  let mut u = User {
    id: v.str("_id").unwrap_or_default(),
    name: v
      .str("name")
      .or_else(|| username.clone())
      .unwrap_or_default(),
    url: username.as_ref().map(|n| format!("{server}/direct/{n}")),
    avatar: username.as_ref().map(|n| format!("{server}/avatar/{n}")),
    bio: v.str("bio").filter(|b| !b.is_empty()),
    created_at: v.at("createdAt").as_str().and_then(parse_time),
    handle: username,
    raw: Some(v.clone()),
    ..User::default()
  };
  for key in ["status", "statusText", "utcOffset"] {
    if let Some(s) = v.str(key).filter(|s| !s.is_empty()) {
      u.extra.insert(key.into(), json!(s));
    }
  }
  if !v.list("roles").is_empty() {
    u.extra.insert("roles".into(), v.at("roles").clone());
  }
  u
}

pub fn room(v: &Value, view: &View) -> Collection {
  let id = v.first_str(&["rid", "_id"]).unwrap_or_default();
  let info = crate::rooms::info_of(v);
  let mut c = Collection {
    url: Some(format!("{}/{}", view.creds.server, info.path)),
    kind: info.kind.clone(),
    name: info.name,
    description: v
      .first_str(&["topic", "description", "announcement"])
      .filter(|d| !d.is_empty()),
    followers: v.u64("usersCount"),
    items: v.u64("msgs"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(n) = v.u64("unread").filter(|n| *n > 0) {
    c.extra.insert("unread".into(), json!(n));
  }
  if let Some(n) = v.u64("userMentions").filter(|n| *n > 0) {
    c.extra.insert("mentions".into(), json!(n));
  }
  if let Some(t) = v.at("ls").as_str().and_then(parse_time) {
    c.extra.insert("last_seen_at".into(), json!(t.to_string()));
  }
  if let Some(t) = v.str("teamId") {
    c.extra.insert("team".into(), json!(t));
  }
  c.id = id;
  c
}

fn media(v: &Value, view: &View) -> Vec<Media> {
  let mut out = Vec::new();
  for a in v.list("attachments") {
    for (key, kind) in [
      ("image_url", MediaKind::Image),
      ("video_url", MediaKind::Video),
      ("audio_url", MediaKind::Audio),
    ] {
      if let Some(url) = a.str(key) {
        let mut m = Media::new(kind, view.absolute(&url));
        m.alt = a.str("title");
        m.size = a.u64("image_size").or_else(|| a.u64("video_size"));
        m.headers = view.creds.headers();
        out.push(m);
      }
    }
  }
  if out.is_empty() {
    // Other files: the message's file list.
    for f in v.list("files") {
      let (Some(id), Some(name)) = (f.str("_id"), f.str("name")) else {
        continue;
      };
      let mut m = Media::new(
        MediaKind::File,
        view.absolute(&format!("/file-upload/{id}/{name}")),
      );
      m.alt = Some(name);
      m.headers = view.creds.headers();
      out.push(m);
    }
  }
  out
}

pub fn message(v: &Value, view: &View) -> Post {
  let id = v.str("_id").unwrap_or_default();
  let rid = v.str("rid").unwrap_or_default();
  let room = view.rooms.get(&rid);
  let mut reactions = Vec::new();
  let mut total = 0;
  if let Some(map) = v.at("reactions").as_object() {
    for (emoji, r) in map {
      let n = r.list("usernames").len() as u64;
      total += n;
      reactions.push(json!({"emoji": emoji.trim_matches(':'), "count": n}));
    }
  }
  let kind = match v.str("t").as_deref() {
    None | Some("") => "message".to_owned(),
    Some("rm") => "delete".to_owned(),
    Some(t) => t.to_owned(),
  };
  let author = v.at("u").is_object().then(|| {
    let mut u = user(v.at("u"), &view.creds.server);
    u.raw = None;
    if let Some(alias) = v.str("alias") {
      u.name = alias;
    }
    u
  });
  let mut p = Post {
    kind,
    text: v.str("msg").filter(|m| !m.is_empty()),
    url: room.map(|r| format!("{}/{}?msg={id}", view.creds.server, r.path)),
    author,
    created_at: time(v.at("ts")),
    updated_at: time(v.at("editedAt")),
    metrics: Metrics {
      likes: (total > 0).then_some(total),
      comments: v.u64("tcount"),
      ..Metrics::default()
    },
    media: media(v, view),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let e = &mut p.extra;
  e.insert("chat".into(), json!(rid));
  if let Some(r) = room {
    e.insert("chat_name".into(), json!(r.name));
  }
  if let Some(t) = v.str("tmid") {
    e.insert("reply_to".into(), json!(t));
  }
  if !reactions.is_empty() {
    e.insert("reactions".into(), json!(reactions));
  }
  if v.bool("pinned") == Some(true) {
    e.insert("pinned".into(), json!(true));
  }
  let notes: Vec<Value> = v
    .list("attachments")
    .iter()
    .filter_map(|a| a.first_str(&["text", "description", "title"]))
    .map(Value::from)
    .collect();
  if !notes.is_empty() {
    e.insert("attachments".into(), json!(notes));
  }
  p.id = id;
  p
}

pub fn comment(v: &Value, view: &View) -> Comment {
  let p = message(v, view);
  Comment {
    id: p.id,
    author: p.author,
    text: p.text.unwrap_or_default(),
    created_at: p.created_at,
    likes: p.metrics.likes,
    extra: p.extra,
    raw: p.raw,
    ..Comment::default()
  }
}
