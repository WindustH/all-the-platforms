//! Posts, their counters and media.

use jiff::Timestamp;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

use super::*;

/// Counters on a post. Platform-only counters (coins, danmaku, quotes, ...) go in `other`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub views: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub likes: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub comments: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub shares: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub favorites: Option<u64>,
  #[serde(flatten)]
  pub other: BTreeMap<String, u64>,
}

impl Metrics {
  pub fn is_empty(&self) -> bool {
    self.views.is_none()
      && self.likes.is_none()
      && self.comments.is_none()
      && self.shares.is_none()
      && self.favorites.is_none()
      && self.other.is_empty()
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
  Image,
  Video,
  Audio,
  Gif,
  /// Any other attachment: documents, archives, stickers ...
  File,
}

#[derive(Debug, Clone, Serialize)]
pub struct Media {
  pub kind: MediaKind,
  pub url: String,
  /// Separate audio track of a split (DASH) video; downloads merge it with ffmpeg.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub audio_url: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub width: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub height: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub duration: Option<f64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub alt: Option<String>,
  /// Bytes of `url` / `audio_url`, when the platform says.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub size: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub audio_size: Option<u64>,
  /// URL query parameter the host takes byte ranges in (`range=a-b`) instead
  /// of the `Range` header; downloads then need `size` / `audio_size`.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub range_param: Option<String>,
  /// Extra request headers the host wants for the download (a cookie the
  /// stream URL is bound to ...); never printed.
  #[serde(skip)]
  pub headers: Vec<(String, String)>,
}

impl Media {
  pub fn new(kind: MediaKind, url: impl Into<String>) -> Self {
    Self {
      kind,
      url: url.into(),
      audio_url: None,
      width: None,
      height: None,
      duration: None,
      alt: None,
      size: None,
      audio_size: None,
      range_param: None,
      headers: Vec::new(),
    }
  }

  pub fn image(url: impl Into<String>) -> Self {
    Self::new(MediaKind::Image, url)
  }

  pub fn video(url: impl Into<String>) -> Self {
    Self::new(MediaKind::Video, url)
  }
}

/// A piece of content: note, tweet, video, answer, article, question, pin, dynamic ...
#[derive(Debug, Clone, Default, Serialize)]
pub struct Post {
  pub id: String,
  /// Platform content type, e.g. `note`, `tweet`, `video`, `answer`, `question`.
  pub kind: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  /// Canonical link. Other commands accept it back as a post reference.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub author: Option<User>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub created_at: Option<Timestamp>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub updated_at: Option<Timestamp>,
  #[serde(skip_serializing_if = "Metrics::is_empty")]
  pub metrics: Metrics,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub media: Vec<Media>,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub tags: Vec<String>,
  /// Quoted / reposted / parent content.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub quoted: Option<Box<Post>>,
  #[serde(skip_serializing_if = "Extra::is_empty")]
  pub extra: Extra,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub raw: Option<Value>,
}

impl Post {
  /// What other commands should receive to refer back to this post.
  pub fn reference(&self) -> &str {
    self.url.as_deref().unwrap_or(&self.id)
  }
}
