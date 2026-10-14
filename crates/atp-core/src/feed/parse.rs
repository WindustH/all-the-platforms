//! feed-rs's model → [`Feed`] with its entries as posts.

use feed_rs::model::{self, FeedType, LinkTarget, MediaObject, Text};
use jiff::Timestamp;

use super::{Feed, not_a_feed};
use crate::error::Result;
use crate::model::{Media, MediaKind, Post, User};
use crate::text::html_to_text;
use crate::{Value, json};

/// Parse a feed document; `url` is where it came from (relative links
/// resolve against it).
pub fn parse(bytes: &[u8], url: &str) -> Result<Feed> {
  let parsed = feed_rs::parser::Builder::new()
    .base_uri(Some(url))
    .build()
    .parse(bytes)
    .map_err(|e| not_a_feed(url, e))?;
  let author = parsed.authors.iter().find_map(|p| p.name.clone());
  let entries = parsed
    .entries
    .iter()
    .map(|entry| post(entry, author.as_deref()))
    .collect();
  Ok(Feed {
    url: url.to_owned(),
    format: match parsed.feed_type {
      FeedType::Atom => "atom",
      FeedType::JSON => "json",
      _ => "rss",
    },
    title: parsed.title.as_ref().map(text),
    link: site(&parsed.links, url),
    description: parsed.description.as_ref().map(text),
    author,
    image: parsed
      .logo
      .as_ref()
      .or(parsed.icon.as_ref())
      .map(|image| image.uri.clone()),
    language: parsed.language.clone(),
    categories: parsed.categories.iter().map(category).collect(),
    updated: time(parsed.updated.or(parsed.published).map(|t| t.timestamp())),
    entries,
  })
}

/// Plain text of a feed text: HTML rendered, other text trimmed.
fn text(text: &Text) -> String {
  if text.content_type.to_string().contains("html") {
    html_to_text(&text.content)
  } else {
    text.content.trim().to_owned()
  }
}

/// Unix seconds (feed-rs hands out chrono times) as a timestamp.
fn time(secs: Option<i64>) -> Option<Timestamp> {
  secs.and_then(|secs| Timestamp::from_second(secs).ok())
}

fn category(category: &model::Category) -> String {
  category
    .label
    .clone()
    .unwrap_or_else(|| category.term.clone())
}

/// The site link of a feed: the first link that is not the feed itself.
fn site(links: &[model::Link], url: &str) -> Option<String> {
  links
    .iter()
    .find(|link| link.rel.as_deref() != Some("self") && link.href != url)
    .map(|link| link.href.clone())
}

/// An entry's own page: its `alternate` link, else the first that is not
/// about comments or the feed.
fn entry_link(links: &[model::Link]) -> Option<String> {
  let page = |link: &&model::Link| {
    link.target.is_none() && !matches!(link.rel.as_deref(), Some("self" | "enclosure" | "replies"))
  };
  links
    .iter()
    .filter(page)
    .find(|link| link.rel.as_deref().is_none_or(|rel| rel == "alternate"))
    .or_else(|| links.iter().find(page))
    .map(|link| link.href.clone())
}

fn post(entry: &model::Entry, feed_author: Option<&str>) -> Post {
  let author = entry
    .authors
    .iter()
    .find_map(|p| p.name.clone().map(|name| (name, p.uri.clone())))
    .or_else(|| credit(&entry.media).map(|name| (name, None)))
    .or_else(|| feed_author.map(|name| (name.to_owned(), None)));
  let body = entry
    .content
    .as_ref()
    .and_then(|content| content.body.as_deref())
    .map(html_to_text)
    .or_else(|| entry.summary.as_ref().map(text))
    .or_else(|| {
      entry
        .media
        .iter()
        .find_map(|m| m.description.as_ref())
        .map(text)
    })
    .filter(|body| !body.is_empty());
  let mut post = Post {
    id: entry.id.clone(),
    kind: "entry".into(),
    title: entry
      .title
      .as_ref()
      .map(text)
      .or_else(|| entry.media.iter().find_map(|m| m.title.as_ref()).map(text)),
    text: body,
    url: entry_link(&entry.links),
    author: author.map(|(name, url)| User {
      id: name.clone(),
      name,
      url,
      ..User::default()
    }),
    created_at: time(entry.published.or(entry.updated).map(|t| t.timestamp())),
    updated_at: time(entry.updated.map(|t| t.timestamp())),
    media: media(&entry.media),
    tags: entry.categories.iter().map(category).collect(),
    ..Post::default()
  };
  extras(&mut post, entry);
  post
}

fn credit(objects: &[MediaObject]) -> Option<String> {
  objects
    .iter()
    .flat_map(|m| &m.credits)
    .map(|credit| credit.entity.clone())
    .next()
}

/// Enclosures and media contents as media, with their type, size and length.
fn media(objects: &[MediaObject]) -> Vec<Media> {
  let mut out: Vec<Media> = Vec::new();
  // iTunes puts the duration on its own object, apart from the enclosure.
  let any_duration = objects.iter().find_map(|m| m.duration);
  for object in objects {
    for content in &object.content {
      let Some(url) = &content.url else { continue };
      if out.iter().any(|m| m.url == url.as_str()) {
        continue;
      }
      let mime = content
        .content_type
        .as_ref()
        .map(|t| t.to_string())
        .unwrap_or_default();
      let mut item = Media::new(kind(&mime, url.as_str()), url.as_str());
      item.size = content.size.filter(|size| *size > 0);
      item.width = content.width;
      item.height = content.height;
      item.duration = content
        .duration
        .or(object.duration)
        .or(any_duration)
        .map(|d| d.as_secs_f64())
        .filter(|secs| *secs > 0.0);
      out.push(item);
    }
  }
  out
}

/// The media kind of a MIME type, or of the file extension when there is none.
fn kind(mime: &str, url: &str) -> MediaKind {
  if !mime.is_empty() {
    return MediaKind::from_mime(mime);
  }
  let path = url
    .split(['?', '#'])
    .next()
    .unwrap_or(url)
    .to_ascii_lowercase();
  let ext = path.rsplit('.').next().unwrap_or_default();
  match ext {
    "mp3" | "m4a" | "aac" | "ogg" | "opus" | "wav" | "flac" => MediaKind::Audio,
    "mp4" | "m4v" | "mov" | "webm" => MediaKind::Video,
    "jpg" | "jpeg" | "png" | "webp" | "avif" => MediaKind::Image,
    "gif" => MediaKind::Gif,
    _ => MediaKind::File,
  }
}

/// Episode, season, duration, image and comment links in `extra`.
fn extras(post: &mut Post, entry: &model::Entry) {
  let mut put = |key: &str, value: Value| {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  };
  let objects = &entry.media;
  if let Some(episode) = objects.iter().find_map(|m| m.episode.as_ref()) {
    put(
      "episode",
      json!(
        episode
          .display
          .clone()
          .unwrap_or_else(|| episode.number.to_string())
      ),
    );
  }
  if let Some(season) = objects.iter().find_map(|m| m.season.as_ref()) {
    put("season", json!(season.number));
  }
  let duration = objects
    .iter()
    .find_map(|m| m.duration)
    .map(|d| d.as_secs_f64());
  put("duration", json!(duration));
  let image = objects
    .iter()
    .flat_map(|m| &m.thumbnails)
    .map(|thumb| thumb.image.uri.clone())
    .next();
  put("image", json!(image));
  let comments = entry
    .links
    .iter()
    .find(|link| link.target == Some(LinkTarget::Comments))
    .map(|link| link.href.clone());
  put("comments", json!(comments));
}
