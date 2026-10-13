//! The feeds YouTube publishes per channel and playlist
//! (`/feeds/videos.xml?channel_id=` / `?playlist_id=`), as NewPipe's feed
//! extractor reads them: the latest 15 entries with exact publication times
//! and view counts, where listings only say "3 days ago".

use std::sync::LazyLock;

use atp_core::{Error, Media, Metrics, Post, Result, User};
use jiff::Timestamp;
use regex::Regex;

use crate::api::{Api, WWW};
use crate::{channels, refs};

static ENTRY: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(?s)<entry>(.*?)</entry>").expect("valid"));

/// Text of the first `<tag>…</tag>` (or the attribute `attr` of `<tag …/>`).
fn field(xml: &str, tag: &str, attr: Option<&str>) -> Option<String> {
  let open = format!("<{tag}");
  let at = xml.find(&open)? + open.len();
  let rest = &xml[at..];
  if !rest.starts_with([' ', '>', '/']) {
    return field(rest, tag, attr);
  }
  let end = rest.find('>')?;
  match attr {
    Some(name) => {
      let head = &rest[..end];
      let key = format!("{name}=\"");
      let start = head.find(&key)? + key.len();
      let len = head[start..].find('"')?;
      Some(unescape(&head[start..start + len]))
    }
    None => {
      let body = &rest[end + 1..];
      let close = body.find(&format!("</{tag}>"))?;
      Some(unescape(&body[..close]))
    }
  }
}

fn unescape(s: &str) -> String {
  s.replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&amp;", "&")
}

fn entry(xml: &str) -> Option<Post> {
  let id = field(xml, "yt:videoId", None)?;
  let author = User {
    name: field(xml, "name", None).unwrap_or_default(),
    url: field(xml, "uri", None),
    id: field(xml, "yt:channelId", None).unwrap_or_default(),
    ..User::default()
  };
  let mut post = Post {
    url: Some(refs::video_url(&id)),
    kind: "video".into(),
    title: field(xml, "title", None),
    text: field(xml, "media:description", None).filter(|d| !d.is_empty()),
    author: Some(author),
    created_at: field(xml, "published", None).and_then(|t| t.parse::<Timestamp>().ok()),
    updated_at: field(xml, "updated", None).and_then(|t| t.parse::<Timestamp>().ok()),
    metrics: Metrics {
      views: field(xml, "media:statistics", Some("views")).and_then(|v| v.parse().ok()),
      likes: field(xml, "media:starRating", Some("count")).and_then(|v| v.parse().ok()),
      ..Metrics::default()
    },
    id,
    ..Post::default()
  };
  if let Some(url) = field(xml, "media:thumbnail", Some("url")) {
    post.media.push(Media {
      alt: Some("thumbnail".into()),
      ..Media::image(url)
    });
  }
  // Shorts link to /shorts/ in the feed.
  if field(xml, "link", Some("href")).is_some_and(|h| h.contains("/shorts/")) {
    post.kind = "short".into();
    post.url = Some(refs::short_url(&post.id));
  }
  Some(post)
}

/// The feed of a channel (id, @handle or link) or of a playlist (`list=`
/// link or id when `playlist` is set).
pub async fn feed(api: &Api, arg: &str, playlist: bool) -> Result<Vec<Post>> {
  let query = if playlist {
    ("playlist_id", refs::playlist(arg)?)
  } else {
    ("channel_id", channels::id(api, arg).await?)
  };
  let resp = api
    .ctx
    .http
    .get(format!("{WWW}/feeds/videos.xml"))
    .query(query.0, &query.1)
    .header("accept", "application/atom+xml, application/xml")
    .send()
    .await?;
  if resp.status.as_u16() == 404 {
    return Err(Error::not_found(format!("no feed for {}", query.1)));
  }
  let xml = resp.check()?.text();
  Ok(
    ENTRY
      .captures_iter(&xml)
      .filter_map(|m| entry(&m[1]))
      .collect(),
  )
}
