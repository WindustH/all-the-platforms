//! The mobile post page: `<script id="RENDER_DATA">` holds the post as
//! URI-encoded JSON (`articleInfo`, `sessionConfig`).

use std::sync::LazyLock;

use atp_core::text::{from_secs, html_to_text};
use atp_core::{Error, Media, Metrics, Post, Result, User, Value, ValueExt};
use regex::Regex;

use super::{kind_name, url_of};
use crate::refs::{Kind, PostRef};

static RENDER_DATA: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r#"(?s)<script id="RENDER_DATA" type="application/json">(.*?)</script>"#)
    .expect("valid")
});
static IMAGE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"<img[^>]+src="(https?://[^"]+)""#).expect("valid"));

/// The page's data, or why there is none.
pub fn render_data(html: &str) -> Result<Value> {
  let encoded = RENDER_DATA
    .captures(html)
    .map(|m| m[1].to_owned())
    .ok_or_else(|| Error::not_found("no post on this page (deleted, or not a Toutiao post)"))?;
  let decoded = url::form_urlencoded::parse(format!("x={encoded}").as_bytes())
    .next()
    .map(|(_, v)| v.into_owned())
    .unwrap_or_default();
  serde_json::from_str(&decoded).map_err(|e| Error::upstream(format!("unreadable page data: {e}")))
}

/// The post of a page; its video (if any) is resolved separately from
/// `extra.play_token`.
pub fn post(data: &Value, wanted: &PostRef) -> Result<Post> {
  let info = data.at("articleInfo");
  let id = info
    .first_str(&["gid"])
    .or_else(|| data.str("sessionConfig.groupId"))
    .unwrap_or_else(|| wanted.id.clone());
  if id != wanted.id {
    return Err(Error::not_found(format!(
      "Toutiao served another post ({id}) for {}",
      wanted.id
    )));
  }
  let kind = kind(data, info, wanted.kind);
  let content = info.str("content").unwrap_or_default();
  let mut post = Post {
    url: Some(url_of(kind, &id)),
    id,
    kind: kind_name(kind).into(),
    title: info.str("title").filter(|t| !t.is_empty()),
    text: Some(html_to_text(&content)).filter(|t| !t.is_empty()),
    author: author(info),
    created_at: info.i64("publishTime").and_then(from_secs),
    metrics: metrics(info),
    media: images(&content),
    raw: Some(info.clone()),
    ..Post::default()
  };
  extra(&mut post, info);
  Ok(post)
}

fn kind(data: &Value, info: &Value, asked: Kind) -> Kind {
  match data.str("sessionConfig.articleType").as_deref() {
    Some("weitoutiao") => Kind::Thread,
    _ if info.str("playAuthTokenV2").is_some() && asked != Kind::Thread => Kind::Video,
    _ => asked,
  }
}

fn author(info: &Value) -> Option<User> {
  let user = info.at("mediaUser");
  let name = user.str("screenName")?;
  let mut author = User {
    id: user.str("id").unwrap_or_default(),
    name,
    avatar: user.str("avatarUrl"),
    bio: user.str("userAuthInfo.authInfo"),
    verified: user.str("userAuthInfo.authType").is_some(),
    ..User::default()
  };
  author.stats.followers = user.count("followerCount");
  Some(author)
}

fn metrics(info: &Value) -> Metrics {
  let mut metrics = Metrics {
    views: info.first_count(&["videoPlayCount", "impressionCount"]),
    likes: info.count("diggCount"),
    comments: info.count("commentCount"),
    shares: info.count("repostCount"),
    favorites: info.count("repinCount"),
    ..Metrics::default()
  };
  if let Some(impressions) = info.count("impressionCount") {
    metrics.other.insert("impressions".into(), impressions);
  }
  metrics
}

/// Images inline in the body, in order, without repeats.
fn images(content: &str) -> Vec<Media> {
  let mut urls: Vec<String> = Vec::new();
  for m in IMAGE.captures_iter(content) {
    let url = m[1].replace("&amp;", "&");
    if !urls.contains(&url) {
      urls.push(url);
    }
  }
  urls.into_iter().map(Media::image).collect()
}

fn extra(post: &mut Post, info: &Value) {
  for (key, path) in [
    ("source", "source"),
    ("source_url", "url"),
    ("biz_tag", "bizTag"),
    ("video_id", "videoId"),
    ("play_token", "playAuthTokenV2"),
    ("poster", "posterUrl"),
  ] {
    if let Some(v) = info.str(path).filter(|v| !v.is_empty()) {
      post.extra.insert(key.into(), v.into());
    }
  }
  if let Some(original) = info.bool("isOriginal") {
    post.extra.insert("original".into(), original.into());
  }
}
