//! Hacker News payloads → the shared model. Stories, jobs, polls and
//! comments are posts (`kind` is the item's type), comments are comments,
//! users are users. Firebase items and Algolia hits name the same things
//! differently; texts are HTML.

use atp_core::text::{from_secs, html_to_text};
use atp_core::{Comment, Metrics, Post, User, UserStats, Value, ValueExt, json};

use crate::api::SITE;

pub fn item_url(id: &str) -> String {
  format!("{SITE}/item?id={id}")
}

pub fn user_url(name: &str) -> String {
  format!("{SITE}/user?id={name}")
}

/// A name as an author.
pub fn author(name: &str) -> User {
  User {
    id: name.to_owned(),
    name: name.to_owned(),
    url: Some(user_url(name)),
    ..User::default()
  }
}

fn html(v: &Value, path: &str) -> Option<String> {
  v.str(path)
    .map(|text| html_to_text(&text))
    .filter(|text| !text.is_empty())
}

/// Whether an item is gone or flagged dead.
pub fn hidden(v: &Value) -> bool {
  v.bool("deleted") == Some(true) || v.bool("dead") == Some(true)
}

/// The linked page and its domain, into `extra`.
fn link(post: &mut Post, url: Option<String>) {
  let Some(url) = url.filter(|u| !u.is_empty()) else {
    return;
  };
  if let Some(host) = url::Url::parse(&url)
    .ok()
    .and_then(|u| u.host_str().map(str::to_owned))
  {
    post
      .extra
      .insert("domain".into(), json!(host.trim_start_matches("www.")));
  }
  post.extra.insert("link".into(), json!(url));
}

/// A Firebase item.
pub fn item(v: &Value) -> Post {
  let id = v.str("id").unwrap_or_default();
  let mut post = Post {
    url: Some(item_url(&id)),
    kind: v.str("type").unwrap_or_else(|| "story".into()),
    title: html(v, "title"),
    text: html(v, "text"),
    author: v.str("by").map(|name| author(&name)),
    created_at: v.i64("time").and_then(from_secs),
    metrics: Metrics {
      likes: v.u64("score"),
      comments: v.u64("descendants"),
      ..Metrics::default()
    },
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  link(&mut post, v.str("url"));
  if let Some(parent) = v.str("parent") {
    post.extra.insert("parent_id".into(), json!(parent));
  }
  post
}

/// An Algolia search hit: a story, or a comment with its story's title.
pub fn hit(v: &Value) -> Post {
  let id = v.str("objectID").unwrap_or_default();
  let tags: Vec<&str> = v.list("_tags").iter().filter_map(Value::as_str).collect();
  let kind = ["comment", "poll", "job", "story"]
    .into_iter()
    .find(|kind| tags.contains(kind))
    .unwrap_or("story");
  let mut post = Post {
    url: Some(item_url(&id)),
    kind: kind.into(),
    title: html(v, "title").or_else(|| html(v, "story_title")),
    text: html(v, "story_text").or_else(|| html(v, "comment_text")),
    author: v.str("author").map(|name| author(&name)),
    created_at: v.i64("created_at_i").and_then(from_secs),
    metrics: Metrics {
      likes: v.u64("points"),
      comments: v.u64("num_comments"),
      ..Metrics::default()
    },
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  link(&mut post, v.str("url").or_else(|| v.str("story_url")));
  if kind == "comment" {
    for (key, path) in [("story_id", "story_id"), ("parent_id", "parent_id")] {
      if let Some(id) = v.str(path) {
        post.extra.insert(key.into(), json!(id));
      }
    }
  }
  post
}

/// A Firebase comment; its replies are counted, not loaded.
pub fn item_comment(v: &Value) -> Comment {
  let id = v.str("id").unwrap_or_default();
  Comment {
    author: v.str("by").map(|name| author(&name)),
    text: html(v, "text").unwrap_or_default(),
    created_at: v.i64("time").and_then(from_secs),
    reply_count: Some(v.list("kids").len() as u64),
    extra: [("url".to_owned(), json!(item_url(&id)))].into(),
    raw: Some(v.clone()),
    id,
    ..Comment::default()
  }
}

/// A node of an Algolia comment tree with every reply under it, oldest
/// first; `None` for a deleted comment without replies.
pub fn tree_comment(v: &Value) -> Option<Comment> {
  let replies: Vec<Comment> = v.list("children").iter().filter_map(tree_comment).collect();
  let text = html(v, "text");
  if text.is_none() && v.str("author").is_none() && replies.is_empty() {
    return None;
  }
  let id = v.str("id").unwrap_or_default();
  let raw: serde_json::Map<String, Value> = v
    .as_object()
    .into_iter()
    .flatten()
    .filter(|(key, _)| *key != "children")
    .map(|(key, value)| (key.clone(), value.clone()))
    .collect();
  Some(Comment {
    author: v.str("author").map(|name| author(&name)),
    text: text.unwrap_or_else(|| "[deleted]".into()),
    created_at: v.i64("created_at_i").and_then(from_secs),
    reply_count: Some(replies.len() as u64),
    replies,
    extra: [("url".to_owned(), json!(item_url(&id)))].into(),
    raw: Some(Value::Object(raw)),
    id,
    ..Comment::default()
  })
}

pub fn user(v: &Value) -> User {
  let name = v.str("id").unwrap_or_default();
  User {
    url: Some(user_url(&name)),
    bio: html(v, "about"),
    created_at: v.i64("created").and_then(from_secs),
    stats: UserStats {
      posts: Some(v.list("submitted").len() as u64),
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
  }
}
