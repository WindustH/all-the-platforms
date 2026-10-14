//! Douban payloads → the shared model. Films, shows, books and albums are
//! posts whose `kind` is their type (`movie`, `tv`, `book`, `music`); a
//! user's marks (想看 / 看过 …) are subjects with `extra.collection`.
//! Reviews, group topics and groups are posts too; short comments (短评)
//! and replies are comments.

mod html;
mod social;

use atp_core::text::{date_only, from_beijing};
use atp_core::{Media, Metrics, Post, Value, ValueExt, json};
use jiff::Timestamp;

pub use html::{chart, search};
pub use social::{comment, group, interest, review, topic, user};

/// How many tags a subject shows.
const TAGS: usize = 12;

/// `2026-08-28(中国大陆)` or `2006-1-1` as a date; partial dates give none.
/// A subject's time is its earliest release.
pub(super) fn date(text: &str) -> Option<Timestamp> {
  let day: String = text
    .chars()
    .take_while(|c| c.is_ascii_digit() || *c == '-')
    .collect();
  let mut parts = day.split('-').map(|p| p.parse::<i16>().ok());
  let (Some(Some(y)), Some(Some(m)), Some(Some(d))) = (parts.next(), parts.next(), parts.next())
  else {
    return None;
  };
  jiff::civil::Date::new(y, m as i8, d as i8)
    .ok()
    .and_then(date_only)
}

/// A Douban time (`2008-02-27 21:43:23`, Beijing time).
pub fn time(v: &Value, path: &str) -> Option<Timestamp> {
  v.str(path).as_deref().and_then(from_beijing)
}

/// Names of a list of `{name}` objects (directors, actors) or strings.
fn names(v: &Value, path: &str) -> Vec<String> {
  v.list(path)
    .iter()
    .filter_map(|x| x.str("name").or_else(|| x.as_str().map(str::to_owned)))
    .collect()
}

/// The canonical link of a subject: its own `url`, else the site page.
fn subject_url(v: &Value, kind: &str, id: &str) -> String {
  v.str("url")
    .filter(|u| u.contains("douban.com/subject/"))
    .unwrap_or_else(|| {
      let site = match kind {
        "book" => "book",
        "music" => "music",
        _ => "movie",
      };
      format!("https://{site}.douban.com/subject/{id}/")
    })
}

fn image(v: &Value) -> Option<Media> {
  v.first_str(&["pic.large", "cover.url", "cover_url", "pic.normal", "cover"])
    .filter(|u| u.starts_with("http"))
    .map(Media::image)
}

fn tags(v: &Value) -> Vec<String> {
  let mut tags = v
    .list("genres")
    .iter()
    .filter_map(|g| g.as_str().map(str::to_owned))
    .collect::<Vec<_>>();
  for tag in v.list("tags").iter().filter_map(|t| t.str("name")) {
    if tags.len() < TAGS && !tags.contains(&tag) {
      tags.push(tag);
    }
  }
  tags
}

/// A film, show, book or album, from its own payload or a list entry.
pub fn subject(v: &Value) -> Post {
  let id = v.str("id").unwrap_or_default();
  let kind = v.str("type").unwrap_or_else(|| "movie".into());
  let metrics = Metrics {
    comments: v.u64("comment_count"),
    ..Metrics::default()
  };
  let mut post = Post {
    url: Some(subject_url(v, &kind, &id)),
    title: v.str("title"),
    text: v
      .first_str(&["intro", "description", "abstract"])
      .map(|t| t.trim().to_owned())
      .filter(|t| !t.is_empty()),
    created_at: v
      .list("pubdate")
      .iter()
      .filter_map(Value::as_str)
      .filter_map(date)
      .min(),
    metrics,
    media: image(v).into_iter().collect(),
    tags: tags(v),
    raw: Some(v.clone()),
    kind,
    id,
    ..Post::default()
  };
  if let Some(reviews) = v.u64("review_count") {
    post.metrics.other.insert("reviews".into(), reviews);
  }
  subject_extra(&mut post, v);
  post
}

fn subject_extra(post: &mut Post, v: &Value) {
  let rating = v.at("rating");
  let honor = v.at("honor_infos.0");
  let values = [
    ("original_title", json!(v.str("original_title"))),
    ("year", json!(v.str("year"))),
    ("rating", json!(rating.f64("value").filter(|r| *r > 0.0))),
    ("votes", json!(rating.u64("count").filter(|n| *n > 0))),
    ("subtitle", json!(v.first_str(&["card_subtitle", "info"]))),
    ("directors", json!(names(v, "directors"))),
    ("actors", json!(names(v, "actors"))),
    ("author", json!(names(v, "author"))),
    ("singer", json!(names(v, "singer"))),
    ("press", json!(names(v, "press"))),
    ("countries", json!(names(v, "countries"))),
    ("durations", json!(names(v, "durations"))),
    ("pubdate", json!(names(v, "pubdate"))),
    (
      "episodes",
      json!(v.u64("episodes_count").filter(|n| *n > 0)),
    ),
    (
      "honor",
      json!(
        honor
          .str("title")
          .map(|t| format!("{t} #{}", honor.str("rank").unwrap_or_default()))
      ),
    ),
    ("null_rating_reason", json!(v.str("null_rating_reason"))),
  ];
  for (key, value) in values {
    let empty = value.is_null() || value.as_array().is_some_and(Vec::is_empty);
    if !empty {
      post.extra.insert(key.into(), value);
    }
  }
  if let Some(mine) = v.get("interest").filter(|i| i.is_object()) {
    post.extra.insert("interest".into(), mark(mine));
  }
}

/// A mark of a subject: state, stars, short comment, tags, time.
fn mark(v: &Value) -> Value {
  let tags: Vec<&str> = v.list("tags").iter().filter_map(Value::as_str).collect();
  json!({
    "status": v.str("status").map(|s| status_name(&s)),
    "rating": v.u64("rating.value"),
    "comment": v.str("comment"),
    "tags": tags,
    "time": v.str("create_time"),
  })
}

/// Douban's state (`mark`, `doing`, `done`) as `--status` names it.
pub fn status_name(status: &str) -> String {
  match status {
    "mark" => "wish".into(),
    other => other.to_owned(),
  }
}

/// One entry of a ranked list (榜单): the subject with its place in `extra.rank`.
pub fn ranked(v: &Value, rank: usize) -> Post {
  let mut post = subject(v);
  post.extra.insert("rank".into(), json!(rank));
  if let Some(text) = v.str("description").filter(|t| !t.is_empty()) {
    post.text.get_or_insert(text);
  }
  post
}

/// One subject a user marked: the subject, with the mark in
/// `extra.collection`; its time is when it was marked (the release date
/// moves to `extra.date`).
pub fn collected(v: &Value) -> Post {
  let mut post = subject(v.at("subject"));
  if let Some(date) = post.created_at.take() {
    post.extra.insert("date".into(), json!(date.to_string()));
  }
  post.created_at = time(v, "create_time");
  post.extra.insert("collection".into(), mark(v));
  post.raw = Some(v.clone());
  post
}
