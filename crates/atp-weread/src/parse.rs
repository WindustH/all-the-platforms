//! WeRead payloads → the shared model. Books are posts (`kind` `book`; an
//! official account's articles shelved as one book are `mp`, audio albums
//! `album`) with their detail page as `url`; public reviews (点评) are
//! comments; your highlights (划线) and thoughts (想法) are posts of kind
//! `highlight` and `thought`.

use atp_core::text::{date_only, from_secs, html_to_text};
use atp_core::{Comment, Media, Post, Price, User, Value, ValueExt, json};

use crate::refs::book_url;

/// `2025-09-01 00:00:00` as the date it names.
fn publish_date(v: &Value) -> Option<jiff::Timestamp> {
  let text = v.str("publishTime")?;
  let day = text.split_whitespace().next()?;
  day.parse::<jiff::civil::Date>().ok().and_then(date_only)
}

fn kind_of(id: &str, v: &Value) -> &'static str {
  if id.starts_with("MP_WXS_") {
    "mp"
  } else if v.at("albumInfo").is_object() || v.str("albumId").is_some() {
    "album"
  } else {
    "book"
  }
}

/// A book from its detail, a search or list entry (`bookInfo`) or the shelf.
pub fn book(v: &Value) -> Post {
  let id = v.str("bookId").unwrap_or_default();
  let categories: Vec<String> = v
    .list("categories")
    .iter()
    .filter_map(|c| c.str("title"))
    .collect();
  let mut post = Post {
    url: Some(book_url(&id)),
    kind: kind_of(&id, v).into(),
    title: v.str("title"),
    text: v
      .str("intro")
      .map(|t| t.trim().to_owned())
      .filter(|t| !t.is_empty()),
    created_at: publish_date(v),
    price: v.f64("price").filter(|p| *p > 0.0).map(Price::cny),
    media: v.str("cover").map(Media::image).into_iter().collect(),
    tags: if categories.is_empty() {
      v.str("category").into_iter().collect()
    } else {
      categories
    },
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  if let Some(readers) = v.u64("readingCount") {
    post.metrics.other.insert("readers".into(), readers);
  }
  if let Some(ratings) = v.u64("newRatingCount").filter(|n| *n > 0) {
    post.metrics.other.insert("ratings".into(), ratings);
  }
  book_extra(&mut post, v);
  post
}

fn book_extra(post: &mut Post, v: &Value) {
  // `newRating` is the share of readers who recommend it, in tenths of a percent.
  let rating = v
    .u64("newRating")
    .filter(|r| *r > 0)
    .map(|r| r as f64 / 10.0);
  let values = [
    ("author", json!(v.str("author"))),
    ("translator", json!(v.str("translator"))),
    ("publisher", json!(v.str("publisher"))),
    ("isbn", json!(v.str("isbn"))),
    ("recommend_percent", json!(rating)),
    ("rating_title", json!(v.str("newRatingDetail.title"))),
    ("words", json!(v.u64("totalWords").filter(|n| *n > 0))),
    ("format", json!(v.str("format"))),
    ("finished", json!(v.bool("finished"))),
    ("free", json!(v.bool("free").filter(|f| *f))),
  ];
  for (key, value) in values {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  }
}

/// An entry of a ranking or category list, with its place.
pub fn ranked(v: &Value, rank: u64) -> Post {
  let mut post = book(v.at("bookInfo"));
  post
    .extra
    .insert("rank".into(), json!(v.u64("searchIdx").unwrap_or(rank)));
  if let Some(readers) = v.u64("readingCount") {
    post.metrics.other.insert("readers".into(), readers);
  }
  post.raw = Some(v.clone());
  post
}

/// A book on your shelf, with how far you are (`extra.progress`, percent)
/// and when you last read it (its time).
pub fn shelved(v: &Value, progress: Option<&Value>) -> Post {
  let mut post = book(v);
  post.created_at = v
    .i64("readUpdateTime")
    .or_else(|| v.i64("updateTime"))
    .and_then(from_secs);
  if v.bool("finishReading") == Some(true) {
    post.extra.insert("finished_reading".into(), json!(true));
  }
  if v.bool("secret") == Some(true) {
    post.extra.insert("private".into(), json!(true));
  }
  if let Some(p) = progress {
    post
      .extra
      .insert("progress".into(), json!(p.u64("progress")));
    if let Some(seconds) = p.u64("readingTime").filter(|s| *s > 0) {
      post.extra.insert("reading_seconds".into(), json!(seconds));
    }
  }
  post
}

pub fn user(v: &Value) -> User {
  let id = v.str("userVid").unwrap_or_default();
  User {
    name: v.str("name").unwrap_or_else(|| id.clone()),
    avatar: v.str("avatar"),
    bio: v.str("signature").filter(|s| !s.trim().is_empty()),
    location: v.str("location").filter(|s| !s.is_empty()),
    followed: v.bool("isFollowing"),
    raw: Some(v.clone()),
    id,
    ..User::default()
  }
}

/// A review in a list: `{review: {review: {…}}}`, `{review: {…}}` or bare.
fn inner(v: &Value) -> &Value {
  [v.at("review.review"), v.at("review")]
    .into_iter()
    .find(|r| r.is_object())
    .unwrap_or(v)
}

/// A public review (点评 / 书评) of a book.
pub fn review(v: &Value) -> Comment {
  let r = inner(v);
  let text = r
    .str("htmlContent")
    .map(|h| html_to_text(&h))
    .or_else(|| r.str("content"))
    .unwrap_or_default();
  let mut comment = Comment {
    id: r.str("reviewId").unwrap_or_default(),
    author: r.at("author").is_object().then(|| user(r.at("author"))),
    text,
    created_at: r.i64("createTime").and_then(from_secs),
    likes: r.u64("likesCount"),
    reply_count: r.u64("commentsCount"),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  for (key, value) in [
    ("stars", json!(r.u64("star").filter(|s| *s > 0))),
    ("rating", json!(rating_name(r.u64("newRatingLevel")))),
    ("finished", json!(r.bool("isFinish").filter(|f| *f))),
  ] {
    if !value.is_null() {
      comment.extra.insert(key.into(), value);
    }
  }
  comment
}

/// WeRead's three verdicts (`newRatingLevel`).
fn rating_name(level: Option<u64>) -> Option<&'static str> {
  match level? {
    1 => Some("recommend"),
    2 => Some("fair"),
    3 => Some("poor"),
    _ => None,
  }
}

/// Where a note sits: `chapterUid` and the start of its `range` (`12-34`).
pub fn position(v: &Value) -> (u64, u64) {
  let start = v
    .str("range")
    .and_then(|r| r.split('-').next().and_then(|s| s.parse().ok()))
    .unwrap_or(0);
  (v.u64("chapterUid").unwrap_or(0), start)
}

/// One of your highlights (划线).
pub fn highlight(v: &Value, chapter: Option<&str>) -> Post {
  let mut post = Post {
    id: v.str("bookmarkId").unwrap_or_default(),
    kind: "highlight".into(),
    text: v.str("markText"),
    created_at: v.i64("createTime").and_then(from_secs),
    raw: Some(v.clone()),
    ..Post::default()
  };
  note_extra(&mut post, v, chapter);
  post
}

/// One of your thoughts (想法) on a passage, or your review (点评) of the
/// book; the passage is in `extra.quote`.
pub fn thought(v: &Value, chapter: Option<&str>) -> Post {
  let r = inner(v);
  let mut post = Post {
    id: r.str("reviewId").unwrap_or_default(),
    kind: if r.u64("type") == Some(4) {
      "review"
    } else {
      "thought"
    }
    .into(),
    text: r.str("content"),
    created_at: r.i64("createTime").and_then(from_secs),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(quote) = r.str("abstract").filter(|q| !q.trim().is_empty()) {
    post.extra.insert("quote".into(), json!(quote));
  }
  let chapter = chapter.map(str::to_owned).or_else(|| r.str("chapterName"));
  note_extra(&mut post, r, chapter.as_deref());
  post
}

fn note_extra(post: &mut Post, v: &Value, chapter: Option<&str>) {
  if let Some(chapter) = chapter {
    post.extra.insert("chapter".into(), json!(chapter));
  }
  if let Some(range) = v.str("range") {
    post.extra.insert("range".into(), json!(range));
  }
  if let Some(book) = v.str("bookId") {
    post.extra.insert("book".into(), json!(book));
  }
}
