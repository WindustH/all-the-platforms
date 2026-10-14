//! V2EX payloads (API 1.0 and 2.0 name things alike) → the shared model:
//! topics are posts, replies are comments (with their floor), members are
//! users, nodes are collections, notifications are notifications.

use std::sync::LazyLock;

use atp_core::text::{from_secs, html_to_text};
use atp_core::{Collection, Comment, Metrics, Notification, Post, User, Value, ValueExt, json};
use regex::Regex;

use crate::api::BASE;

static TOPIC_LINK: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"href="(/t/\d+[^"]*)""#).unwrap());

/// The source text (Markdown or plain), else the rendered HTML as text.
fn content(v: &Value) -> Option<String> {
  v.str("content")
    .map(|text| text.replace("\r\n", "\n").trim().to_owned())
    .or_else(|| v.str("content_rendered").map(|html| html_to_text(&html)))
    .filter(|text| !text.is_empty())
}

fn time(v: &Value, path: &str) -> Option<jiff::Timestamp> {
  v.i64(path).and_then(from_secs)
}

pub fn topic_url(id: &str) -> String {
  format!("{BASE}/t/{id}")
}

pub fn member(v: &Value) -> User {
  let name = v.str("username").unwrap_or_default();
  let mut user = User {
    url: Some(format!("{BASE}/member/{name}")),
    avatar: v.first_str(&["avatar_large", "avatar", "avatar_normal"]),
    bio: v.str("bio").map(|bio| bio.replace("\r\n", "\n")),
    location: v.str("location"),
    created_at: time(v, "created"),
    raw: Some(v.clone()),
    id: name.clone(),
    name,
    ..User::default()
  };
  for key in ["website", "github", "twitter", "tagline", "psn", "btc"] {
    if let Some(value) = v.str(key) {
      user.extra.insert(key.into(), json!(value));
    }
  }
  if let Some(id) = v.u64("id") {
    user.extra.insert("number".into(), json!(id));
  }
  if v.bool("pro") == Some(true) {
    user.extra.insert("pro".into(), json!(true));
  }
  user
}

pub fn topic(v: &Value) -> Post {
  let id = v.str("id").unwrap_or_default();
  let member = v.at("member");
  let node = v.at("node");
  let mut post = Post {
    url: Some(topic_url(&id)),
    kind: "topic".into(),
    title: v.str("title"),
    text: content(v),
    author: (!member.is_null()).then(|| member_brief(member)),
    created_at: time(v, "created"),
    updated_at: time(v, "last_modified"),
    metrics: Metrics {
      comments: v.u64("replies"),
      ..Metrics::default()
    },
    tags: node.str("name").into_iter().collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  if !node.is_null() {
    post.extra.insert(
      "node".into(),
      json!({"name": node.str("name"), "title": node.str("title")}),
    );
  }
  for (key, value) in [
    ("last_reply_by", json!(v.str("last_reply_by"))),
    ("last_touched_at", json!(time(v, "last_touched"))),
  ] {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  }
  let supplements: Vec<String> = v.list("supplements").iter().filter_map(content).collect();
  if !supplements.is_empty() {
    post.extra.insert("supplements".into(), json!(supplements));
  }
  post
}

/// A member inside a topic or reply: the profile fields without `raw`.
fn member_brief(v: &Value) -> User {
  User {
    raw: None,
    ..member(v)
  }
}

/// A reply; `floor` is its number in the topic (1 for the first reply).
pub fn reply(v: &Value, floor: usize, topic: &str) -> Comment {
  let member = v.at("member");
  let mut comment = Comment {
    id: v.str("id").unwrap_or_default(),
    author: (!member.is_null()).then(|| member_brief(member)),
    text: content(v).unwrap_or_default(),
    created_at: time(v, "created"),
    likes: v.u64("thanks"),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  comment.extra.insert("floor".into(), json!(floor));
  comment.extra.insert(
    "url".into(),
    json!(format!("{}#r_{}", topic_url(topic), comment.id)),
  );
  comment
}

pub fn node(v: &Value) -> Collection {
  let name = v.str("name").unwrap_or_default();
  let mut node = Collection {
    url: Some(format!("{BASE}/go/{name}")),
    kind: "node".into(),
    name: v.str("title").unwrap_or_else(|| name.clone()),
    description: v
      .str("header")
      .map(|header| html_to_text(&header))
      .filter(|header| !header.is_empty()),
    items: v.u64("topics"),
    followers: v.u64("stars"),
    raw: Some(v.clone()),
    id: name,
    ..Collection::default()
  };
  for key in ["title_alternative", "parent_node_name"] {
    if let Some(value) = v.str(key) {
      node.extra.insert(key.into(), json!(value));
    }
  }
  node
}

/// A notification: its HTML sentence as text, the kind read from it.
pub fn notification(v: &Value) -> Notification {
  let html = v.str("text").unwrap_or_default();
  let text = html_to_text(&html);
  let kind = [
    ("回复", "reply"),
    ("提到", "mention"),
    ("感谢", "like"),
    ("收藏", "favorite"),
  ]
  .into_iter()
  .find(|(word, _)| text.contains(word))
  .map_or("system", |(_, kind)| kind);
  let username = v.str("member.username");
  Notification {
    id: v.str("id").unwrap_or_default(),
    kind: kind.into(),
    text,
    actor: username.map(|name| User {
      id: name.clone(),
      url: Some(format!("{BASE}/member/{name}")),
      name,
      ..User::default()
    }),
    target: v
      .str("payload_rendered")
      .map(|html| html_to_text(&html))
      .or_else(|| v.str("payload"))
      .filter(|payload| !payload.is_empty()),
    url: TOPIC_LINK
      .captures(&html)
      .and_then(|c| c.get(1))
      .map(|path| format!("{BASE}{}", path.as_str())),
    created_at: time(v, "created"),
    raw: Some(v.clone()),
    ..Notification::default()
  }
}
