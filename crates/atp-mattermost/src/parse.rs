//! Mattermost payloads → the shared model: posts with permalinks, channels
//! and teams as collections, files as media fetched with the token.

use std::collections::HashMap;

use atp_core::text::from_millis;
use atp_core::{
  Collection, Comment, Extra, Media, MediaKind, Metrics, Post, User, Value, ValueExt, json,
};

use crate::chats::ChannelInfo;
use crate::users::Brief;

pub struct View<'a> {
  pub server: &'a str,
  pub token: &'a str,
  pub me: &'a str,
  pub users: &'a HashMap<String, Brief>,
  pub channels: &'a HashMap<String, ChannelInfo>,
  pub teams: &'a HashMap<String, String>,
}

impl View<'_> {
  /// A team name for links: the given team's, or any (direct channels
  /// belong to none, and every team's links reach them).
  fn team(&self, id: Option<&str>) -> Option<&str> {
    id.and_then(|t| self.teams.get(t))
      .or_else(|| self.teams.values().next())
      .map(String::as_str)
  }

  pub fn post_url(&self, channel: &str, post: &str) -> Option<String> {
    let team = self.channels.get(channel).and_then(|c| c.team.as_deref());
    self
      .team(team)
      .map(|t| format!("{}/{t}/pl/{post}", self.server))
  }
}

pub fn team(v: &Value, server: &str) -> Collection {
  let name = v.str("name").unwrap_or_default();
  let mut c = Collection {
    id: v.str("id").unwrap_or_default(),
    kind: "team".into(),
    name: v.str("display_name").unwrap_or_else(|| name.clone()),
    description: v.str("description").filter(|d| !d.is_empty()),
    url: Some(format!("{server}/{name}")),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  c.extra.insert("name".into(), json!(name));
  c
}

pub fn channel(v: &Value, view: &View) -> Collection {
  let id = v.str("id").unwrap_or_default();
  let info = crate::chats::info_of(v, view.me, view.users);
  let slug = v.str("name").unwrap_or_default();
  let url = view
    .team(info.team.as_deref())
    .map(|t| match info.kind.as_str() {
      "dm" => format!(
        "{}/{t}/messages/@{}",
        view.server,
        info.name.trim_start_matches('@')
      ),
      "group" => format!("{}/{t}/messages/{slug}", view.server),
      _ => format!("{}/{t}/channels/{slug}", view.server),
    });
  let mut c = Collection {
    kind: info.kind.clone(),
    name: info.name,
    description: v
      .first_str(&["purpose", "header"])
      .filter(|d| !d.is_empty()),
    url,
    items: v.u64("total_msg_count"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(t) = info.team.as_deref().and_then(|t| view.teams.get(t)) {
    c.extra.insert("team".into(), json!(t));
  }
  if let Some(t) = v.i64("last_post_at").and_then(from_millis) {
    c.extra.insert("last_post_at".into(), json!(t.to_string()));
  }
  c.extra.insert("slug".into(), json!(slug));
  c.id = id;
  c
}

fn author(v: &Value, view: &View) -> Option<User> {
  let id = v.str("user_id")?;
  let mut u = view.users.get(&id).map(|b| b.user(&id)).unwrap_or(User {
    name: id.clone(),
    id: id.clone(),
    ..User::default()
  });
  if let Some(name) = v.str("props.override_username") {
    u.name = name;
  }
  Some(u)
}

/// The kind of media a MIME type names.
fn media_kind(mime: &str) -> MediaKind {
  match mime.split('/').next() {
    _ if mime == "image/gif" => MediaKind::Gif,
    Some("image") => MediaKind::Image,
    Some("video") => MediaKind::Video,
    Some("audio") => MediaKind::Audio,
    _ => MediaKind::File,
  }
}

fn file(f: &Value, view: &View) -> Option<Media> {
  let id = f.str("id")?;
  let mime = f.str("mime_type").unwrap_or_default();
  let mut m = Media::new(
    media_kind(&mime),
    format!("{}/api/v4/files/{id}", view.server),
  );
  m.width = f.u64("width").filter(|w| *w > 0).map(|w| w as u32);
  m.height = f.u64("height").filter(|h| *h > 0).map(|h| h as u32);
  m.size = f.u64("size");
  m.alt = f.str("name");
  m.headers = vec![("authorization".into(), format!("Bearer {}", view.token))];
  Some(m)
}

/// How often each emoji was used, in first-use order.
fn reaction_counts(reactions: &[Value]) -> Vec<(String, u64)> {
  let mut counts: Vec<(String, u64)> = Vec::new();
  for r in reactions {
    let name = r.str("emoji_name").unwrap_or_default();
    match counts.iter_mut().find(|(n, _)| *n == name) {
      Some((_, c)) => *c += 1,
      None => counts.push((name, 1)),
    }
  }
  counts
}

pub fn post(v: &Value, view: &View) -> Post {
  let id = v.str("id").unwrap_or_default();
  let channel = v.str("channel_id").unwrap_or_default();
  let reactions = v.list("metadata.reactions");
  let kind = match v.str("type").as_deref() {
    None | Some("") => "message".to_owned(),
    Some(t) => t.trim_start_matches("system_").to_owned(),
  };
  let files = v.list("metadata.files");
  let mut p = Post {
    kind,
    text: v.str("message").filter(|m| !m.is_empty()),
    url: view.post_url(&channel, &id),
    author: author(v, view),
    created_at: v.i64("create_at").and_then(from_millis),
    updated_at: v.i64("edit_at").filter(|t| *t > 0).and_then(from_millis),
    metrics: Metrics {
      likes: (!reactions.is_empty()).then_some(reactions.len() as u64),
      comments: v.u64("reply_count").filter(|n| *n > 0),
      ..Metrics::default()
    },
    media: files.iter().filter_map(|f| file(f, view)).collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  post_extra(&mut p.extra, v, &channel, view);
  p.id = id;
  p
}

/// Chat, thread root, reactions, pin and attachment texts.
fn post_extra(e: &mut Extra, v: &Value, channel: &str, view: &View) {
  e.insert("chat".into(), json!(channel));
  if let Some(c) = view.channels.get(channel) {
    e.insert("chat_name".into(), json!(c.name));
  }
  if let Some(root) = v.str("root_id").filter(|r| !r.is_empty()) {
    e.insert("reply_to".into(), json!(root));
  }
  let counts = reaction_counts(v.list("metadata.reactions"));
  if !counts.is_empty() {
    let r: Vec<Value> = counts
      .iter()
      .map(|(n, c)| json!({"emoji": n, "count": c}))
      .collect();
    e.insert("reactions".into(), json!(r));
  }
  if v.bool("is_pinned") == Some(true) {
    e.insert("pinned".into(), json!(true));
  }
  let attachments: Vec<Value> = v
    .list("props.attachments")
    .iter()
    .filter_map(|a| a.first_str(&["fallback", "text", "title"]))
    .map(Value::from)
    .collect();
  if !attachments.is_empty() {
    e.insert("attachments".into(), json!(attachments));
  }
}

pub fn comment(v: &Value, view: &View) -> Comment {
  let p = post(v, view);
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

/// Posts of a `{order, posts}` list, in its order.
pub fn ordered(list: &Value) -> Vec<Value> {
  list
    .list("order")
    .iter()
    .filter_map(|id| id.as_str())
    .filter_map(|id| {
      let p = list.at("posts").at(id);
      (!p.is_null()).then(|| p.clone())
    })
    .collect()
}
