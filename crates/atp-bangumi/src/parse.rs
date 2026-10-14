//! Bangumi payloads → the shared model: subjects, characters, persons and
//! episodes are posts (`kind` says which; subjects take their type:
//! `anime`, `book`, `music`, `game`, `real`), users are users, and a
//! user's collection entries are subjects with `extra.collection`.

use atp_core::text::date_only;
use atp_core::{Media, Metrics, Post, User, Value, ValueExt, json};
use jiff::Timestamp;

use crate::api::SITE;

/// Subject types by their number, as `--filter` / `--type` name them.
pub const TYPES: &[(u64, &str)] = &[
  (2, "anime"),
  (1, "book"),
  (3, "music"),
  (4, "game"),
  (6, "real"),
];
/// Collection states by their number, as `--status` names them.
pub const STATUSES: &[(u64, &str)] = &[
  (1, "wish"),
  (2, "done"),
  (3, "doing"),
  (4, "on-hold"),
  (5, "dropped"),
];
/// Episode types by their number (`episodes --type`).
pub const EPISODE_TYPES: &[&str] = &["main", "sp", "op", "ed", "pv", "mad", "other"];
/// How many tags a subject shows.
const TAGS: usize = 12;

pub fn type_name(code: Option<u64>) -> &'static str {
  TYPES
    .iter()
    .find(|(n, _)| Some(*n) == code)
    .map_or("subject", |(_, name)| name)
}

/// The number of a subject type or collection state named on the command line.
pub fn code(table: &[(u64, &str)], name: &str) -> Option<u64> {
  table
    .iter()
    .find(|(_, n)| *n == name)
    .map(|(code, _)| *code)
}

fn status_name(code: Option<u64>) -> Option<&'static str> {
  STATUSES
    .iter()
    .find(|(n, _)| Some(*n) == code)
    .map(|(_, name)| *name)
}

/// `2023-09-29` as a date without a time of day.
fn date(text: Option<String>) -> Option<Timestamp> {
  text?.parse::<jiff::civil::Date>().ok().and_then(date_only)
}

fn text(v: &Value, paths: &[&str]) -> Option<String> {
  v.first_str(paths)
    .map(|text| text.replace("\r\n", "\n").trim().to_owned())
    .filter(|text| !text.is_empty())
}

/// An image of a subject, character or person (Bangumi still hands out
/// some `http://` links).
fn image(v: &Value) -> Option<Media> {
  let url = v.first_str(&["images.large", "images.common", "images.medium", "image"])?;
  Some(Media::image(url.replacen("http://", "https://", 1)))
}

/// `infobox` rows as `{key: value}`; lists become one line.
fn infobox(v: &Value) -> Value {
  let mut map = serde_json::Map::new();
  for row in v.list("infobox") {
    let Some(key) = row.str("key") else { continue };
    let value = match row.at("value") {
      Value::Array(items) => items
        .iter()
        .filter_map(|item| match (item.str("k"), item.str("v")) {
          (Some(k), Some(v)) => Some(format!("{k}: {v}")),
          (_, v) => v,
        })
        .collect::<Vec<_>>()
        .join(", "),
      other => other.as_str().unwrap_or_default().to_owned(),
    };
    if !value.is_empty() {
      map.insert(key, json!(value));
    }
  }
  Value::Object(map)
}

pub fn subject(v: &Value) -> Post {
  let id = v.str("id").unwrap_or_default();
  let name = v.str("name");
  let collection = v.at("collection");
  let mut metrics = Metrics {
    favorites: v.u64("collection_total"),
    ..Metrics::default()
  };
  for key in ["wish", "collect", "doing", "on_hold", "dropped"] {
    if let Some(n) = collection.u64(key) {
      metrics.other.insert(key.into(), n);
    }
  }
  let mut tags: Vec<String> = v
    .list("meta_tags")
    .iter()
    .filter_map(Value::as_str)
    .map(str::to_owned)
    .collect();
  for tag in v.list("tags").iter().filter_map(|t| t.str("name")) {
    if tags.len() < TAGS && !tags.contains(&tag) {
      tags.push(tag);
    }
  }
  let mut post = Post {
    url: Some(format!("{SITE}/subject/{id}")),
    kind: type_name(v.u64("type")).into(),
    title: v.str("name_cn").or_else(|| name.clone()),
    text: text(v, &["summary", "short_summary"]),
    created_at: date(v.first_str(&["date", "air_date"])),
    metrics,
    tags,
    media: image(v).into_iter().collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  subject_extra(&mut post, v, name);
  post
}

fn subject_extra(post: &mut Post, v: &Value, name: Option<String>) {
  let rating = v.at("rating");
  let score = rating.f64("score").or_else(|| v.f64("score"));
  let values = [
    ("name", json!(name)),
    ("platform", json!(v.str("platform"))),
    ("date", json!(v.first_str(&["date", "air_date"]))),
    (
      "eps",
      json!(v.u64("total_episodes").or_else(|| v.u64("eps"))),
    ),
    ("volumes", json!(v.u64("volumes").filter(|n| *n > 0))),
    ("weekday", json!(v.u64("air_weekday"))),
    ("score", json!(score.filter(|s| *s > 0.0))),
    (
      "rank",
      json!(
        rating
          .u64("rank")
          .or_else(|| v.u64("rank"))
          .filter(|r| *r > 0)
      ),
    ),
    ("votes", json!(rating.u64("total"))),
    ("nsfw", json!(v.bool("nsfw").filter(|nsfw| *nsfw))),
  ];
  for (key, value) in values {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  }
  let info = infobox(v);
  if info.as_object().is_some_and(|map| !map.is_empty()) {
    post.extra.insert("infobox".into(), info);
  }
}

/// A character or person of a subject's cast and staff lists, or one on
/// its own page (`kind`: `character` / `person`).
pub fn figure(v: &Value, kind: &str) -> Post {
  let id = v.str("id").unwrap_or_default();
  let mut post = Post {
    url: Some(format!("{SITE}/{kind}/{id}")),
    kind: kind.into(),
    title: v.str("name"),
    text: text(v, &["summary", "short_summary"]),
    media: image(v).into_iter().collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  let actors: Vec<Value> = v
    .list("actors")
    .iter()
    .map(|a| json!({"id": a.str("id"), "name": a.str("name"), "url": format!("{SITE}/person/{}", a.str("id").unwrap_or_default())}))
    .collect();
  let careers: Vec<&str> = v.list("career").iter().filter_map(Value::as_str).collect();
  let values = [
    ("relation", json!(v.str("relation"))),
    ("eps", json!(v.str("eps"))),
    (
      "actors",
      if actors.is_empty() {
        Value::Null
      } else {
        json!(actors)
      },
    ),
    (
      "career",
      if careers.is_empty() {
        Value::Null
      } else {
        json!(careers)
      },
    ),
    ("gender", json!(v.str("gender"))),
    ("collects", json!(v.u64("stat.collects"))),
    ("comments", json!(v.u64("stat.comments"))),
  ];
  for (key, value) in values {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  }
  let info = infobox(v);
  if info.as_object().is_some_and(|map| !map.is_empty()) {
    post.extra.insert("infobox".into(), info);
  }
  post
}

/// A subject related to another one (`extra.relation`: sequel, adaptation …).
pub fn relation(v: &Value) -> Post {
  let mut post = subject(v);
  if let Some(relation) = v.str("relation") {
    post.extra.insert("relation".into(), json!(relation));
  }
  post
}

pub fn episode(v: &Value) -> Post {
  let id = v.str("id").unwrap_or_default();
  let number = v.str("ep").or_else(|| v.str("sort"));
  let name = v.str("name_cn").or_else(|| v.str("name"));
  let title = match (&number, name) {
    (Some(n), Some(name)) => format!("{n}. {name}"),
    (Some(n), None) => format!("Episode {n}"),
    (None, name) => name.unwrap_or_default(),
  };
  let kind = v
    .u64("type")
    .and_then(|t| EPISODE_TYPES.get(t as usize))
    .copied();
  let mut post = Post {
    url: Some(format!("{SITE}/ep/{id}")),
    kind: "episode".into(),
    title: Some(title),
    text: text(v, &["desc"]),
    created_at: date(v.str("airdate")),
    metrics: Metrics {
      comments: v.u64("comment"),
      ..Metrics::default()
    },
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  let values = [
    ("number", json!(number)),
    ("sort", json!(v.f64("sort"))),
    ("type", json!(kind)),
    ("name", json!(v.str("name"))),
    (
      "duration",
      json!(v.u64("duration_seconds").filter(|s| *s > 0)),
    ),
    ("subject", json!(v.str("subject_id"))),
  ];
  for (key, value) in values {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  }
  post
}

pub fn user(v: &Value) -> User {
  let name = v.str("username").unwrap_or_default();
  let mut user = User {
    url: Some(format!("{SITE}/user/{name}")),
    name: v.str("nickname").unwrap_or_else(|| name.clone()),
    handle: Some(name.clone()),
    avatar: v.first_str(&["avatar.large", "avatar.medium"]),
    bio: v.str("sign").filter(|sign| !sign.trim().is_empty()),
    raw: Some(v.clone()),
    id: name,
    ..User::default()
  };
  if let Some(number) = v.u64("id") {
    user.extra.insert("number".into(), json!(number));
  }
  user
}

/// One entry of a user's collection: the subject, with the state, rating,
/// progress, comment and tags in `extra.collection`; its time is when the
/// entry last changed (the release date moves to `extra.date`).
pub fn collected(v: &Value) -> Post {
  let mut post = subject(v.at("subject"));
  let updated = v
    .str("updated_at")
    .and_then(|t| t.parse::<Timestamp>().ok());
  if let Some(date) = post.created_at.take() {
    post
      .extra
      .entry("date".into())
      .or_insert(json!(date.to_string()));
  }
  post.created_at = updated;
  let tags: Vec<String> = v
    .list("tags")
    .iter()
    .filter_map(Value::as_str)
    .map(str::to_owned)
    .collect();
  post.extra.insert(
    "collection".into(),
    json!({
      "status": status_name(v.u64("type")),
      "rating": v.u64("rate").filter(|r| *r > 0),
      "episodes": v.u64("ep_status"),
      "volumes": v.u64("vol_status").filter(|n| *n > 0),
      "comment": v.str("comment"),
      "tags": tags,
      "private": v.bool("private"),
    }),
  );
  post.raw = Some(v.clone());
  post
}
