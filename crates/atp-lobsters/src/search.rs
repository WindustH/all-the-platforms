//! Search. Lobsters' search page has no JSON version, so its story list is
//! read from the HTML: one `<li id="story_<id>">` per result with the
//! title link, score, tags, submitter, time and comment count.

use std::sync::LazyLock;

use atp_core::text::{from_secs, html_to_text};
use atp_core::{Metrics, Page, PageReq, Post, Query, Result, json};
use regex::Regex;

use crate::api::{Api, BASE};
use crate::parse;

/// `search --sort`.
pub const SORTS: &[&str] = &["relevance", "newest", "score"];

static STORY: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"<li id="story_([a-z0-9]+)""#).unwrap());
static TITLE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"<a class="u-url" href="([^"]+)"[^>]*>([^<]*)</a>"#).unwrap());
static SCORE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"class="upvoter"[^>]*>\s*(-?\d+)\s*<"#).unwrap());
static TAG: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"class="tag [^"]*"[^>]*href="/t/([^"]+)""#).unwrap());
static AUTHOR: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"class="u-author[^"]*" href="/~([^"]+)""#).unwrap());
static TIME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"data-at-unix="(\d+)""#).unwrap());
static COMMENTS: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"href="(/s/[a-z0-9]+/[^"]*)">\s*(\d+)\s+comments?"#).unwrap());

pub async fn stories(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let no = page.number_or(1);
  let order = query.sort.clone().unwrap_or_else(|| "relevance".into());
  let params = vec![
    ("q", query.keyword.clone()),
    ("what", "stories".to_owned()),
    ("order", order),
    ("page", no.to_string()),
  ];
  let html = api.html("search", params).await?;
  let posts = blocks(&html)
    .into_iter()
    .map(|(id, block)| story(id, block))
    .collect();
  let next = html
    .contains(&format!("page={}&", no + 1))
    .then(|| (no + 1).to_string());
  Ok(Page::new(posts, next))
}

/// Each result's short id and HTML.
fn blocks(html: &str) -> Vec<(&str, &str)> {
  let starts: Vec<(usize, &str)> = STORY
    .captures_iter(html)
    .filter_map(|c| Some((c.get(0)?.start(), c.get(1)?.as_str())))
    .collect();
  starts
    .iter()
    .enumerate()
    .map(|(i, (start, id))| {
      let end = starts.get(i + 1).map_or(html.len(), |(next, _)| *next);
      (*id, &html[*start..end])
    })
    .collect()
}

fn story(id: &str, block: &str) -> Post {
  let first = |re: &Regex| {
    re.captures(block)
      .and_then(|c| c.get(1))
      .map(|m| m.as_str().to_owned())
  };
  let title = TITLE.captures(block);
  let href = title
    .as_ref()
    .and_then(|c| c.get(1))
    .map(|m| html_to_text(m.as_str()));
  let comments = COMMENTS.captures(block);
  let mut post = Post {
    id: id.to_owned(),
    kind: "story".into(),
    title: title
      .as_ref()
      .and_then(|c| c.get(2))
      .map(|m| html_to_text(m.as_str())),
    url: Some(comments.as_ref().and_then(|c| c.get(1)).map_or_else(
      || format!("{BASE}/s/{id}"),
      |m| format!("{BASE}{}", m.as_str()),
    )),
    author: first(&AUTHOR).map(|name| parse::author(&name)),
    created_at: first(&TIME)
      .and_then(|t| t.parse().ok())
      .and_then(from_secs),
    metrics: Metrics {
      likes: first(&SCORE).and_then(|s| s.parse().ok()),
      comments: Some(
        comments
          .as_ref()
          .and_then(|c| c.get(2))
          .map_or(0, |m| m.as_str().parse().unwrap_or(0)),
      ),
      ..Metrics::default()
    },
    tags: TAG
      .captures_iter(block)
      .filter_map(|c| Some(c.get(1)?.as_str().to_owned()))
      .collect(),
    ..Post::default()
  };
  if let Some(href) = href.filter(|h| h.starts_with("http")) {
    post.extra.insert("link".into(), json!(href));
  }
  post
}
