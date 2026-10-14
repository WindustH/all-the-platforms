//! Lobsters payloads → the shared model: stories are posts (the external
//! link in `extra.link`), comments are comments (rebuilt into threads from
//! their `depth` / `parent_comment`), users are users, tags are
//! collections.

use std::collections::HashMap;

use atp_core::text::{html_to_text, parse_time};
use atp_core::{Collection, Comment, Metrics, Post, User, UserStats, Value, ValueExt, json};

use crate::api::BASE;

/// A user named by a payload: a plain name (current API) or an object
/// with `username` (older one).
fn name_at(v: &Value, path: &str) -> Option<String> {
  v.str(path).or_else(|| v.str(&format!("{path}.username")))
}

pub fn author(name: &str) -> User {
  User {
    id: name.to_owned(),
    name: name.to_owned(),
    url: Some(format!("{BASE}/~{name}")),
    ..User::default()
  }
}

/// Text from the plain copy when there is one, else from the HTML.
fn text(v: &Value, plain: &str, html: &str) -> Option<String> {
  v.str(plain)
    .or_else(|| v.str(html).map(|h| html_to_text(&h)))
    .map(|t| t.replace("\r\n", "\n").trim().to_owned())
    .filter(|t| !t.is_empty())
}

pub fn story(v: &Value) -> Post {
  let id = v.str("short_id").unwrap_or_default();
  let mut post = Post {
    url: v.str("comments_url").or_else(|| v.str("short_id_url")),
    kind: "story".into(),
    title: v.str("title"),
    text: text(v, "description_plain", "description"),
    author: name_at(v, "submitter_user").map(|name| author(&name)),
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    metrics: Metrics {
      likes: v.u64("score"),
      comments: v.u64("comment_count"),
      ..Metrics::default()
    },
    tags: v
      .list("tags")
      .iter()
      .filter_map(|tag| tag.as_str().map(str::to_owned))
      .collect(),
    raw: Some(without_comments(v)),
    id,
    ..Post::default()
  };
  if let Some(link) = v.str("url") {
    if let Some(host) = url::Url::parse(&link)
      .ok()
      .and_then(|u| u.host_str().map(str::to_owned))
    {
      post
        .extra
        .insert("domain".into(), json!(host.trim_start_matches("www.")));
    }
    post.extra.insert("link".into(), json!(link));
  }
  if v.bool("user_is_author") == Some(true) {
    post.extra.insert("by_author".into(), json!(true));
  }
  post
}

fn without_comments(v: &Value) -> Value {
  let mut raw = v.clone();
  if let Some(map) = raw.as_object_mut() {
    map.remove("comments");
  }
  raw
}

pub fn comment(v: &Value) -> Comment {
  let deleted = v.bool("is_deleted") == Some(true);
  let mut comment = Comment {
    id: v.str("short_id").unwrap_or_default(),
    author: name_at(v, "commenting_user").map(|name| author(&name)),
    text: text(v, "comment_plain", "comment").unwrap_or_else(|| "[deleted]".into()),
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    likes: v.u64("score"),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  if let Some(url) = v.str("url").or_else(|| v.str("short_id_url")) {
    comment.extra.insert("url".into(), json!(url));
  }
  if deleted || v.bool("is_moderated") == Some(true) {
    comment.extra.insert("removed".into(), json!(true));
  }
  comment
}

/// A comment read on its own, as a post.
pub fn comment_post(v: &Value) -> Post {
  let mut comment = comment(v);
  comment.extra.remove("url");
  Post {
    id: comment.id,
    kind: "comment".into(),
    text: Some(comment.text),
    url: v.str("url").or_else(|| v.str("short_id_url")),
    author: comment.author,
    created_at: comment.created_at,
    metrics: Metrics {
      likes: comment.likes,
      ..Metrics::default()
    },
    extra: comment.extra,
    raw: comment.raw,
    ..Post::default()
  }
}

/// The flat comment list of a story (in thread order) as threads.
pub fn threads(rows: &[Value]) -> Vec<Comment> {
  let mut nodes: Vec<Option<Comment>> = rows.iter().map(|row| Some(comment(row))).collect();
  let mut index: HashMap<String, usize> = HashMap::new();
  let mut children: Vec<Vec<usize>> = vec![Vec::new(); rows.len()];
  let mut top = Vec::new();
  for (i, row) in rows.iter().enumerate() {
    match row
      .str("parent_comment")
      .and_then(|p| index.get(&p).copied())
    {
      Some(parent) => children[parent].push(i),
      None => top.push(i),
    }
    index.insert(row.str("short_id").unwrap_or_default(), i);
  }
  top
    .into_iter()
    .map(|i| assemble(i, &mut nodes, &children))
    .collect()
}

fn assemble(i: usize, nodes: &mut [Option<Comment>], children: &[Vec<usize>]) -> Comment {
  let mut comment = nodes[i].take().unwrap_or_default();
  comment.replies = children[i]
    .iter()
    .map(|&child| assemble(child, nodes, children))
    .collect();
  comment.reply_count = Some(comment.replies.len() as u64);
  comment
}

pub fn user(v: &Value) -> User {
  let name = v.str("username").unwrap_or_default();
  let mut user = User {
    url: Some(format!("{BASE}/~{name}")),
    avatar: v.str("avatar_url").map(|path| format!("{BASE}{path}")),
    bio: v.str("about").filter(|about| !about.trim().is_empty()),
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    stats: UserStats {
      other: v
        .u64("karma")
        .map(|karma| [("karma".to_owned(), karma)].into())
        .unwrap_or_default(),
      ..UserStats::default()
    },
    raw: Some(v.clone()),
    id: name.clone(),
    name,
    ..User::default()
  };
  for key in ["is_admin", "is_moderator"] {
    if v.bool(key) == Some(true) {
      user.extra.insert(key.into(), json!(true));
    }
  }
  for (key, path) in [
    ("invited_by", "invited_by_user"),
    ("github", "github_username"),
    ("mastodon", "mastodon_username"),
  ] {
    if let Some(value) = name_at(v, path) {
      user.extra.insert(key.into(), json!(value));
    }
  }
  user
}

pub fn tag(v: &Value) -> Collection {
  let name = v.str("tag").unwrap_or_default();
  let mut tag = Collection {
    url: Some(format!("{BASE}/t/{name}")),
    kind: "tag".into(),
    description: v.str("description"),
    raw: Some(v.clone()),
    id: name.clone(),
    name,
    ..Collection::default()
  };
  if let Some(category) = v.str("category") {
    tag.extra.insert("category".into(), json!(category));
  }
  for key in ["privileged", "is_media"] {
    if v.bool(key) == Some(true) {
      tag.extra.insert(key.into(), json!(true));
    }
  }
  tag
}
