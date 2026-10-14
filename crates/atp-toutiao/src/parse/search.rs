//! Search result cards: the search page embeds each result as
//! `<script … type="application/json">{"data": {…}}</script>`.

use std::sync::LazyLock;

use atp_core::text::{from_secs, html_to_text};
use atp_core::{Metrics, Post, User, Value, ValueExt};
use regex::Regex;

use super::{kind_name, kind_of, url_of};

static CARD: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r#"<script[^>]*type="application/json"[^>]*>(\{"data":[\s\S]*?)</script>"#)
    .expect("valid")
});

/// Whether the page is the slide-captcha page instead of results.
pub fn is_captcha(html: &str) -> bool {
  html.contains("window.isCheating=1") || html.contains(r#""type":"verify""#)
}

pub fn cards(html: &str) -> Vec<Post> {
  CARD
    .captures_iter(html)
    .filter_map(|m| serde_json::from_str::<Value>(&m[1]).ok())
    .filter_map(|v| card(v.at("data")))
    .collect()
}

fn card(v: &Value) -> Option<Post> {
  let id = v.first_str(&["group_id", "item_id"])?;
  let kind = kind_of(v);
  let author = v.first_str(&["media_name", "source"]).map(|name| User {
    id: name.clone(),
    name,
    ..User::default()
  });
  let title = v.str("title").map(|t| html_to_text(&t));
  Some(Post {
    url: Some(url_of(kind, &id)),
    id,
    kind: kind_name(kind).into(),
    title,
    text: v
      .first_str(&["abstract", "summary.text"])
      .map(|t| html_to_text(&t)),
    author,
    created_at: v
      .first_str(&["publish_time", "datetime"])
      .and_then(|t| t.parse().ok())
      .and_then(from_secs),
    metrics: Metrics {
      views: v.count("read_count"),
      likes: v.count("digg_count"),
      comments: v.count("comment_count"),
      ..Metrics::default()
    },
    raw: Some(v.clone()),
    ..Post::default()
  })
}
