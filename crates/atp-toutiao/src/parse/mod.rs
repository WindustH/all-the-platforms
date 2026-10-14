//! Toutiao payloads → the shared model. Feed cells come in three dialects:
//! the signed feeds' cells, the legacy channel feed's and 微头条 cells whose
//! payload is a JSON string; [`article`] reads the mobile pages and
//! [`comments`] the comment threads.

pub mod article;
pub mod comments;
pub mod search;

use atp_core::text::from_secs;
use atp_core::{Media, Metrics, Post, User, Value, ValueExt};

use crate::api::WWW;
use crate::refs::{self, Kind};

/// The author of a feed cell (`user_info`, or `user.info` on short
/// videos), whose `user_id` is the `MS4w…` token the user feed takes
/// (short videos carry the numeric id instead).
pub fn user(v: &Value) -> Option<User> {
  let name = v.str("name")?;
  let token = v.str("user_id").filter(|id| id.starts_with("MS4w"));
  Some(User {
    id: v.str("user_id").unwrap_or_default(),
    name,
    url: token.as_deref().map(refs::user_url),
    avatar: v.str("avatar_url"),
    bio: v
      .first_str(&["description", "desc"])
      .filter(|d| !d.is_empty()),
    verified: v.bool("user_verified").unwrap_or(false),
    raw: Some(v.clone()),
    ..User::default()
  })
}

fn metrics(v: &Value) -> Metrics {
  Metrics {
    views: v.first_count(&["read_count", "video_play_count"]),
    likes: v.first_count(&["digg_count", "like_count"]),
    comments: v.first_count(&["comment_count", "comments_count"]),
    shares: v.count("share_count"),
    favorites: v.count("repin_count"),
    ..Metrics::default()
  }
}

pub fn kind_of(v: &Value) -> Kind {
  if v.bool("has_video").unwrap_or(false) || v.i64("cell_type") == Some(49) {
    Kind::Video
  } else {
    Kind::Article
  }
}

pub fn url_of(kind: Kind, id: &str) -> String {
  format!("{WWW}/{}/{id}/", kind.segment())
}

/// A cell of the signed recommend feed or user feed.
pub fn post(v: &Value) -> Option<Post> {
  let id = v.first_str(&["group_id", "item_id_str", "item_id"])?;
  let kind = kind_of(v);
  let image = v.str("middle_image.url").map(Media::image);
  Some(Post {
    url: Some(url_of(kind, &id)),
    id,
    kind: kind_name(kind).into(),
    title: v.str("title").filter(|t| !t.is_empty()),
    text: v
      .first_str(&["Abstract", "abstract"])
      .filter(|t| !t.is_empty()),
    author: user(v.at("user_info")).or_else(|| user(v.at("user.info"))),
    created_at: v.i64("publish_time").and_then(from_secs),
    metrics: metrics(v),
    media: image.into_iter().collect(),
    raw: Some(v.clone()),
    ..Post::default()
  })
}

/// A cell of the legacy channel feed (`/api/pc/feed/`).
pub fn channel_post(v: &Value) -> Option<Post> {
  if v.bool("is_feed_ad").unwrap_or(false) {
    return None;
  }
  let id = v.first_str(&["group_id", "item_id"])?;
  let kind = kind_of(v);
  let author = v.str("source").map(|name| User {
    id: name.clone(),
    name,
    avatar: v.str("media_avatar_url"),
    ..User::default()
  });
  Some(Post {
    url: Some(url_of(kind, &id)),
    id,
    kind: kind_name(kind).into(),
    title: v.str("title"),
    text: v.str("abstract").filter(|t| !t.is_empty()),
    author,
    created_at: v.i64("behot_time").and_then(from_secs),
    metrics: metrics(v),
    raw: Some(v.clone()),
    ..Post::default()
  })
}

/// A 微头条 cell (`stream_cell.raw_data`, a JSON string).
pub fn thread_post(cell: &Value) -> Option<Post> {
  let raw: Value = serde_json::from_str(&cell.str("stream_cell.raw_data")?).ok()?;
  let id = raw.str("thread_id_str")?;
  let author = raw.str("user.name").map(|name| User {
    id: raw.str("user.id").unwrap_or_default(),
    name,
    avatar: raw.str("user.avatar_url"),
    ..User::default()
  });
  let media = raw
    .list("large_image_list")
    .iter()
    .filter_map(|i| i.first_str(&["url_list.0.url", "url"]))
    .map(Media::image)
    .collect();
  Some(Post {
    url: Some(url_of(Kind::Thread, &id)),
    id,
    kind: "thread".into(),
    text: raw.str("content"),
    author,
    created_at: raw.i64("create_time").and_then(from_secs),
    metrics: metrics(&raw),
    media,
    raw: Some(raw),
    ..Post::default()
  })
}

/// One entry of the hot board: a topic with its heat.
pub fn hot(v: &Value) -> Option<Post> {
  let id = v.first_str(&["ClusterIdStr", "ClusterId"])?;
  let mut metrics = Metrics::default();
  if let Some(heat) = v.count("HotValue") {
    metrics.other.insert("heat".into(), heat);
  }
  let url = v
    .str("Url")
    .map(|u| u.split('?').next().unwrap_or_default().to_owned());
  let mut post = Post {
    id,
    kind: "topic".into(),
    title: v.str("Title"),
    url,
    metrics,
    media: v.str("Image.url").map(Media::image).into_iter().collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(label) = v.str("Label").filter(|l| !l.is_empty()) {
    post.tags.push(label);
  }
  Some(post)
}

pub fn kind_name(kind: Kind) -> &'static str {
  match kind {
    Kind::Article => "article",
    Kind::Video => "video",
    Kind::Thread => "thread",
  }
}
