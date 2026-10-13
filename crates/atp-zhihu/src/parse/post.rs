//! Content objects: answers, questions, articles, pins and videos (zvideo).

use std::sync::LazyLock;

use atp_core::text::{html_to_text, parse_count};
use atp_core::{Media, Metrics, Post, Value, ValueExt};
use regex::Regex;

use super::{author, text, time};
use crate::api::WWW;
use crate::refs::{answer_url, article_url, question_url};

/// Any supported content object (`answer`, `article`, `question`, `pin`).
pub fn post(v: &Value) -> Option<Post> {
  match v.str("type")?.as_str() {
    "answer" => Some(answer(v)),
    "article" => Some(article(v)),
    "question" => Some(question(v)),
    "pin" => Some(pin(v)),
    "zvideo" => Some(zvideo(v)),
    _ => None,
  }
}

fn base(v: &Value, kind: &str) -> Post {
  Post {
    id: v.str("id").unwrap_or_default(),
    kind: kind.into(),
    author: author(v),
    created_at: time(v, &["created_time", "created"]),
    updated_at: time(v, &["updated_time", "updated"]),
    raw: Some(v.clone()),
    ..Post::default()
  }
}

pub fn answer(v: &Value) -> Post {
  let mut p = base(v, "answer");
  let html = v.first_str(&["content", "excerpt"]).unwrap_or_default();
  let question = v.str("question.id");
  p.title = v
    .first_str(&["question.title", "question.name"])
    .map(|t| html_to_text(&t));
  p.url = Some(answer_url(&p.id, question.as_deref()));
  p.text = text(&html);
  p.media = images(&html);
  p.metrics = Metrics {
    views: v.count("visit_count"),
    likes: v.count("voteup_count"),
    comments: v.count("comment_count"),
    favorites: v.count("favlists_count"),
    ..Metrics::default()
  };
  other(&mut p.metrics, v, &[("thanks", "thanks_count")]);
  statistics(&mut p.metrics, v);
  if let Some(q) = question {
    p.extra.insert("question_id".into(), q.into());
  }
  p
}

pub fn question(v: &Value) -> Post {
  let mut p = base(v, "question");
  p.title = v.first_str(&["title", "name"]).map(|t| html_to_text(&t));
  p.url = Some(question_url(&p.id));
  let html = v.first_str(&["detail", "excerpt"]).unwrap_or_default();
  p.text = text(&html);
  p.media = images(&html);
  p.tags = topics(v);
  p.metrics = Metrics {
    views: v.count("visit_count"),
    likes: v.count("voteup_count"),
    comments: v.count("comment_count"),
    ..Metrics::default()
  };
  other(
    &mut p.metrics,
    v,
    &[("answers", "answer_count"), ("followers", "follower_count")],
  );
  p
}

pub fn article(v: &Value) -> Post {
  let mut p = base(v, "article");
  p.title = v.str("title").map(|t| html_to_text(&t));
  p.url = Some(article_url(&p.id));
  let html = v.first_str(&["content", "excerpt"]).unwrap_or_default();
  p.text = text(&html);
  p.media = v
    .str("image_url")
    .map(Media::image)
    .into_iter()
    .chain(images(&html))
    .collect();
  p.tags = topics(v);
  p.metrics = Metrics {
    likes: v.count("voteup_count"),
    comments: v.count("comment_count"),
    favorites: v.count("favlists_count"),
    ..Metrics::default()
  };
  statistics(&mut p.metrics, v);
  if let Some(column) = v.str("column.title") {
    p.extra.insert("column".into(), column.into());
  }
  p
}

/// A pin (想法): text, images and links in `content`; a repin quotes its origin.
pub fn pin(v: &Value) -> Post {
  let mut p = base(v, "pin");
  p.title = v.str("title").filter(|t| !t.trim().is_empty());
  p.url = Some(format!("{WWW}/pin/{}", p.id));
  let mut html = String::new();
  for item in v.list("content") {
    match item.str("type").as_deref() {
      Some("text") => html.push_str(&item.str("content").unwrap_or_default()),
      Some("image") => {
        if let Some(url) = item.first_str(&["original_url", "url"]) {
          let mut m = Media::image(url);
          m.width = item.u64("width").map(|w| w as u32);
          m.height = item.u64("height").map(|h| h as u32);
          p.media.push(m);
        }
      }
      Some("video") => p.media.extend(video(item.at("video_info"))),
      Some("link") => {
        if let Some(url) = item.str("url") {
          html.push_str(&format!("<p>{url}</p>"));
        }
      }
      _ => {}
    }
  }
  if html.is_empty() {
    html = v
      .first_str(&["content_html", "excerpt_title"])
      .unwrap_or_default();
  }
  p.text = text(&html);
  p.tags = topics(v);
  p.metrics = Metrics {
    // A pin's `like_count` is its old 赞 counter; 赞同 is `reaction_count`.
    likes: v.first_count(&["reaction_count", "like_count"]),
    comments: v.count("comment_count"),
    shares: v.count("repin_count"),
    favorites: v.first_count(&["favlists_count", "favorite_count"]),
    views: v.count("page_view_count").filter(|n| *n > 0),
    ..Metrics::default()
  };
  statistics(&mut p.metrics, v);
  if v.at("origin_pin").is_object() {
    p.quoted = Some(Box::new(pin(v.at("origin_pin"))));
  }
  p
}

/// A video (视频, `zvideo`). Listings carry no stream URL, only the cover.
pub fn zvideo(v: &Value) -> Post {
  let mut p = base(v, "zvideo");
  p.title = v.str("title").filter(|t| !t.trim().is_empty());
  p.url = Some(format!("{WWW}/zvideo/{}", p.id));
  p.text = v
    .first_str(&["description", "excerpt"])
    .and_then(|d| text(&d));
  p.created_at = p.created_at.or_else(|| time(v, &["published_at"]));
  p.updated_at = p.updated_at.or_else(|| time(v, &["updated_at"]));
  p.media = video(v.at("video")).into_iter().collect();
  if p.media.is_empty()
    && let Some(cover) = v.first_str(&["video.thumbnail", "image_url"])
  {
    p.media.push(Media::image(cover));
  }
  p.metrics = Metrics {
    views: v.first_count(&["play_count", "video.play_count"]),
    likes: v.count("voteup_count"),
    comments: v.count("comment_count"),
    favorites: v.count("favlists_count"),
    shares: v.count("share_count"),
    ..Metrics::default()
  };
  other(&mut p.metrics, v, &[("danmaku", "bullet_count")]);
  statistics(&mut p.metrics, v);
  if let Some(d) = v.f64("video.duration") {
    p.extra.insert("duration".into(), d.into());
  }
  p
}

/// The best stream of a `video_info` / `video` object with a `playlist`
/// (`fhd`, `hd`, `sd`, `ld`), when it has one.
fn video(v: &Value) -> Option<Media> {
  let best = ["fhd", "hd", "sd", "ld"]
    .iter()
    .map(|q| v.at(&format!("playlist.{q}")))
    .find(|s| s.first_str(&["play_url", "url"]).is_some())?;
  let mut m = Media::video(best.first_str(&["play_url", "url"])?);
  m.width = best.u64("width").map(|w| w as u32);
  m.height = best.u64("height").map(|h| h as u32);
  m.duration = best.f64("duration").or_else(|| v.f64("duration"));
  m.size = best.u64("size").filter(|s| *s > 0);
  Some(m)
}

/// An entry of the follow feed or of a user's activities: the content, with
/// who did what (`赞同了回答`) and when in `extra`.
pub fn activity(v: &Value) -> Option<Post> {
  let mut p = post(v.at("target"))?;
  let actor = v
    .first_str(&["actors.0.name", "actor.name"])
    .filter(|n| !n.is_empty());
  if let Some(verb) = v.str("verb") {
    p.extra.insert("action".into(), verb.into());
  }
  if let Some(text) = v.str("action_text") {
    p.extra.insert("action_text".into(), text.into());
  }
  if let Some(actor) = actor {
    p.extra.insert("actor".into(), actor.into());
  }
  if let Some(at) = time(v, &["created_time"]) {
    p.extra.insert("acted_at".into(), at.to_string().into());
  }
  Some(p)
}

/// A read-history card (`unify-consumption/read_history`): what was read and when.
pub fn history_item(v: &Value) -> Option<Post> {
  let d = v.at("data");
  let extra = d.at("extra");
  let id = extra.str("content_token")?;
  let kind = extra.str("content_type").unwrap_or_else(|| "answer".into());
  let question = extra.str("question_token");
  let url = match kind.as_str() {
    "answer" => answer_url(&id, question.as_deref()),
    "article" => article_url(&id),
    "question" => question_url(&id),
    "pin" => format!("{WWW}/pin/{id}"),
    "zvideo" => format!("{WWW}/zvideo/{id}"),
    _ => d
      .str("header.action.url")
      .unwrap_or_else(|| format!("{WWW}/{kind}/{id}")),
  };
  let mut p = Post {
    id,
    kind,
    title: d.str("header.title"),
    text: d.str("content.summary"),
    url: Some(url),
    author: d.str("content.author_name").map(|name| atp_core::User {
      name,
      ..Default::default()
    }),
    media: d
      .str("content.cover_image")
      .map(Media::image)
      .into_iter()
      .collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(at) = time(extra, &["read_time"]) {
    p.extra.insert("read_at".into(), at.to_string().into());
  }
  if let Some(q) = question {
    p.extra.insert("question_id".into(), q.into());
  }
  let stats: Vec<String> = d
    .list("matrix")
    .iter()
    .filter_map(|m| m.str("data.text"))
    .collect();
  if !stats.is_empty() {
    p.extra.insert("summary".into(), stats.join(" · ").into());
  }
  Some(p)
}

/// An entry of the hot list: the question with its heat (`1874 万热度`).
pub fn hot_item(v: &Value) -> Option<Post> {
  let mut p = post(v.at("target"))?;
  if let Some(heat) = v.str("detail_text") {
    if let Some(n) = parse_count(heat.trim_end_matches("热度")) {
      p.metrics.other.insert("heat".into(), n);
    }
    p.extra.insert("heat".into(), heat.into());
  }
  Some(p)
}

fn topics(v: &Value) -> Vec<String> {
  v.list("topics")
    .iter()
    .filter_map(|t| t.str("name"))
    .collect()
}

/// Counters of `reaction.statistics`, which current payloads carry next to (or
/// instead of) the older fields: 喜欢 (`hearts`), down votes, plays, danmaku.
fn statistics(m: &mut Metrics, v: &Value) {
  let s = v.at("reaction.statistics");
  if !s.is_object() {
    return;
  }
  m.likes = m.likes.or_else(|| s.count("up_vote_count"));
  m.comments = m.comments.or_else(|| s.count("comment_count"));
  m.favorites = m.favorites.or_else(|| s.count("favorites"));
  m.shares = m.shares.or_else(|| s.count("share_count"));
  for (key, field, always) in [
    ("hearts", "like_count", true),
    ("down_votes", "down_vote_count", true),
    ("plays", "play_count", false),
    ("danmaku", "bullet_count", false),
    ("applauds", "applaud_count", false),
  ] {
    if let Some(n) = s.count(field).filter(|n| always || *n > 0) {
      m.other.entry(key.into()).or_insert(n);
    }
  }
}

fn other(m: &mut Metrics, v: &Value, fields: &[(&str, &str)]) {
  for (key, field) in fields {
    if let Some(n) = v.count(field) {
      m.other.insert((*key).into(), n);
    }
  }
}

/// Images embedded in a body, preferring the original resolution.
fn images(html: &str) -> Vec<Media> {
  static IMG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<img\b[^>]*>").unwrap());
  static ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"([\w-]+)\s*=\s*"([^"]*)""#).unwrap());
  let mut out: Vec<Media> = Vec::new();
  for tag in IMG.find_iter(html) {
    let attr = |name: &str| {
      ATTR
        .captures_iter(tag.as_str())
        .find(|c| &c[1] == name)
        .map(|c| c[2].replace("&amp;", "&"))
    };
    let Some(url) = ["data-original", "data-actualsrc", "src"]
      .iter()
      .filter_map(|a| attr(a))
      .find(|u| u.starts_with("http") && !u.contains("/equation?"))
    else {
      continue;
    };
    if out.iter().any(|m| m.url == url) {
      continue;
    }
    let mut m = Media::image(url);
    m.width = attr("data-rawwidth").and_then(|w| w.parse().ok());
    m.height = attr("data-rawheight").and_then(|h| h.parse().ok());
    out.push(m);
  }
  out
}
