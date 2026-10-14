//! Feeds and entries → the shared model. An entry is a post linking to
//! its article (`url`; its own reference when it has no link), with that
//! reference in `extra.ref` and its feed in `extra.feed`. A feed is a
//! user (a source of posts), or a post listing its latest entries.

use atp_core::feed::{self, Feed};
use atp_core::text::{fmt_time, html_to_text};
use atp_core::{Media, MediaKind, Post, User, UserStats, Value, ValueExt, json};
use jiff::Timestamp;

use crate::index::REF;
use crate::refs::MINIFLUX_SCHEME;

/// How many entries a feed's overview lists.
const OVERVIEW: usize = 20;

pub fn entry(feed: &Feed, entry: &Post) -> Post {
  let mut post = entry.clone();
  let reference = feed::entry_ref(&feed.url, &entry.id);
  post.url.get_or_insert_with(|| reference.clone());
  post.extra.insert(REF.into(), json!(reference));
  if let Some(title) = &feed.title {
    post.extra.insert("feed".into(), json!(title));
  }
  post.extra.insert("feed_url".into(), json!(feed.url));
  post
}

/// A feed as the source of its entries.
pub fn source(feed: &Feed) -> User {
  let mut user = User {
    id: feed.url.clone(),
    name: feed.title.clone().unwrap_or_else(|| feed.url.clone()),
    url: Some(feed.url.clone()),
    avatar: feed.image.clone(),
    bio: feed.description.clone(),
    stats: UserStats {
      posts: Some(feed.entries.len() as u64),
      ..UserStats::default()
    },
    ..User::default()
  };
  for (key, value) in [
    ("website", json!(feed.link)),
    ("format", json!(feed.format)),
    ("language", json!(feed.language)),
    ("updated_at", json!(feed.updated)),
  ] {
    if !value.is_null() {
      user.extra.insert(key.into(), value);
    }
  }
  user
}

/// A feed with its latest entries: numbered lines in the text, and
/// `extra.entries` with each entry's link and reference.
pub fn overview(feed: &Feed) -> Post {
  let mut entries: Vec<Post> = feed.entries.iter().map(|e| entry(feed, e)).collect();
  entries.sort_by_key(|post| std::cmp::Reverse(post.created_at));
  entries.truncate(OVERVIEW);
  let mut text = feed.description.clone().unwrap_or_default();
  if !text.is_empty() {
    text.push_str("\n\n");
  }
  text.push_str(&format!("Latest of {} entries:", feed.entries.len()));
  for (i, post) in entries.iter().enumerate() {
    let when = post.created_at.map(fmt_time).unwrap_or_default();
    let title = post.title.as_deref().unwrap_or("(untitled)");
    text.push_str(&format!("\n{:>3}. {when:<11} {title}", i + 1));
  }
  let listed: Vec<Value> = entries
    .iter()
    .map(|post| json!({"title": post.title, "url": post.url, "ref": post.extra.get(REF), "published_at": post.created_at}))
    .collect();
  let mut post = Post {
    id: feed.url.clone(),
    kind: "feed".into(),
    title: feed.title.clone(),
    text: Some(text),
    url: Some(feed.url.clone()),
    author: feed.author.clone().map(|name| User {
      id: name.clone(),
      name,
      ..User::default()
    }),
    updated_at: feed.updated,
    tags: feed.categories.clone(),
    ..Post::default()
  };
  for (key, value) in [
    ("website", json!(feed.link)),
    ("format", json!(feed.format)),
    ("entries", json!(listed)),
    ("image", json!(feed.image)),
  ] {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  }
  post
}

fn time(v: &Value, path: &str) -> Option<Timestamp> {
  v.str(path).and_then(|t| t.parse().ok())
}

/// A Miniflux entry.
pub fn miniflux_entry(v: &Value) -> Post {
  let id = v.str("id").unwrap_or_default();
  let reference = format!("{MINIFLUX_SCHEME}{id}");
  let mut post = Post {
    kind: "entry".into(),
    title: v.str("title"),
    text: v
      .str("content")
      .map(|html| html_to_text(&html))
      .filter(|text| !text.is_empty()),
    url: v.str("url").or_else(|| Some(reference.clone())),
    author: v.str("author").map(|name| User {
      id: name.clone(),
      name,
      ..User::default()
    }),
    created_at: time(v, "published_at"),
    updated_at: time(v, "changed_at"),
    tags: v
      .list("tags")
      .iter()
      .filter_map(Value::as_str)
      .map(str::to_owned)
      .collect(),
    media: v
      .list("enclosures")
      .iter()
      .filter_map(|e| {
        let url = e.str("url")?;
        let mut media = Media::new(
          MediaKind::from_mime(&e.str("mime_type").unwrap_or_default()),
          url,
        );
        media.size = e.u64("size").filter(|size| *size > 0);
        Some(media)
      })
      .collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  for (key, value) in [
    (REF, json!(reference)),
    ("feed", json!(v.str("feed.title"))),
    ("feed_url", json!(v.str("feed.feed_url"))),
    ("category", json!(v.str("feed.category.title"))),
    ("status", json!(v.str("status"))),
    ("starred", json!(v.bool("starred"))),
    ("reading_time", json!(v.u64("reading_time"))),
    ("comments", json!(v.str("comments_url"))),
  ] {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  }
  post
}

/// A feed of the Miniflux account, with its unread count.
pub fn miniflux_feed(v: &Value, unread: Option<u64>) -> User {
  let mut user = User {
    id: v.str("id").unwrap_or_default(),
    name: v.str("title").unwrap_or_default(),
    url: v.str("feed_url"),
    raw: Some(v.clone()),
    ..User::default()
  };
  for (key, value) in [
    ("website", json!(v.str("site_url"))),
    ("category", json!(v.str("category.title"))),
    ("unread", json!(unread)),
    ("checked_at", json!(v.str("checked_at"))),
    (
      "errors",
      json!(v.u64("parsing_error_count").filter(|n| *n > 0)),
    ),
  ] {
    if !value.is_null() {
      user.extra.insert(key.into(), value);
    }
  }
  user
}
