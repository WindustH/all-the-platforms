//! What Douban serves as HTML: the mobile search's result fragments and
//! the film chart page (新片榜).

use std::sync::LazyLock;

use atp_core::{Media, Post, json};
use regex::Regex;

fn re(pattern: &str) -> Regex {
  Regex::new(pattern).expect("valid pattern")
}

static LINK: LazyLock<Regex> = LazyLock::new(|| {
  re(
    r#"href="(?:https://[a-z]+\.douban\.com)?/(?:(movie|tv|book|music)/subject/(\d+)|subject/(\d+)|group/([^/"]+))/?""#,
  )
});
static IMG: LazyLock<Regex> = LazyLock::new(|| re(r#"<img[^>]*\ssrc="([^"]+)""#));
static TITLE: LazyLock<Regex> = LazyLock::new(|| re(r#"subject-title">([^<]+)<"#));
static SCORE: LazyLock<Regex> = LazyLock::new(|| re(r"<span>\s*(\d+(?:\.\d+)?)\s*</span>"));
static MEMBERS: LazyLock<Regex> = LazyLock::new(|| re(r"<span>\s*(\d+)\s*人\s*</span>"));
static CHART_TITLE: LazyLock<Regex> = LazyLock::new(|| re(r#"class="nbg"[^>]*title="([^"]+)""#));
static CHART_INFO: LazyLock<Regex> =
  LazyLock::new(|| re(r#"(?s)class="pl2">.*?<p[^>]*>([^<]+)</p>"#));
static CHART_SCORE: LazyLock<Regex> = LazyLock::new(|| re(r#"rating_nums">([\d.]+)<"#));
static CHART_VOTES: LazyLock<Regex> = LazyLock::new(|| re(r"\((\d+)人评价\)"));

fn capture(re: &Regex, text: &str) -> Option<String> {
  re.captures(text)
    .map(|m| unescape(m[1].trim()))
    .filter(|s| !s.is_empty())
}

fn unescape(text: &str) -> String {
  text
    .replace("&amp;", "&")
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&nbsp;", " ")
}

/// What a result links to: `(kind, id)`, a group being `("group", id)`.
fn target(chunk: &str) -> Option<(String, String)> {
  let m = LINK.captures(chunk)?;
  if let Some(group) = m.get(4) {
    return Some(("group".into(), group.as_str().to_owned()));
  }
  let kind = m.get(1).map_or("movie", |k| k.as_str());
  let id = m.get(2).or_else(|| m.get(3))?.as_str();
  Some((kind.to_owned(), id.to_owned()))
}

/// One result of the mobile search (`/j/search/`): a subject or a group.
fn result(chunk: &str) -> Option<Post> {
  let (kind, id) = target(chunk)?;
  let mut post = Post {
    title: capture(&TITLE, chunk),
    media: capture(&IMG, chunk).map(Media::image).into_iter().collect(),
    ..Post::default()
  };
  if kind == "group" {
    post.url = Some(format!("https://www.douban.com/group/{id}/"));
    if let Some(n) = capture(&MEMBERS, chunk).and_then(|n| n.parse().ok()) {
      post.metrics.other.insert("members".into(), n);
    }
  } else {
    let site = if kind == "tv" { "movie" } else { &kind };
    post.url = Some(format!("https://{site}.douban.com/subject/{id}/"));
    if let Some(score) = capture(&SCORE, chunk).and_then(|s| s.parse::<f64>().ok()) {
      post.extra.insert("rating".into(), json!(score));
    }
  }
  post.kind = kind;
  post.id = id;
  Some(post)
}

/// The results in one `html` fragment of the mobile search.
pub fn search(html: &str) -> Vec<Post> {
  html.split("<li").skip(1).filter_map(result).collect()
}

/// The films of the chart page (`movie.douban.com/chart`, 新片榜).
pub fn chart(html: &str) -> Vec<Post> {
  html
    .split(r#"<tr class="item">"#)
    .skip(1)
    .enumerate()
    .filter_map(|(i, chunk)| chart_row(chunk, i + 1))
    .collect()
}

fn chart_row(chunk: &str, rank: usize) -> Option<Post> {
  let (_, id) = target(chunk)?;
  let info = capture(&CHART_INFO, chunk);
  let mut post = Post {
    url: Some(format!("https://movie.douban.com/subject/{id}/")),
    kind: "movie".into(),
    title: capture(&CHART_TITLE, chunk),
    text: info.clone(),
    created_at: info.as_deref().and_then(super::date),
    media: capture(&IMG, chunk).map(Media::image).into_iter().collect(),
    id,
    ..Post::default()
  };
  post.extra.insert("rank".into(), json!(rank));
  if let Some(score) = capture(&CHART_SCORE, chunk).and_then(|s| s.parse::<f64>().ok()) {
    post.extra.insert("rating".into(), json!(score));
  }
  if let Some(votes) = capture(&CHART_VOTES, chunk).and_then(|s| s.parse::<u64>().ok()) {
    post.extra.insert("votes".into(), json!(votes));
  }
  Some(post)
}
