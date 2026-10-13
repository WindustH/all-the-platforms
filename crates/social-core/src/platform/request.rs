//! What commands hand to a platform: paging, queries, drafts, messages,
//! QR logins.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Which page of a listing to fetch.
#[derive(Debug, Clone, Default)]
pub struct PageReq {
  /// Opaque cursor from the previous page's `next_cursor`; `None` for the first page.
  pub cursor: Option<String>,
  /// How many more items the caller wants; a hint for the upstream page size.
  pub size: usize,
}

impl PageReq {
  /// Upstream page size: the wanted count clamped to `[1, max]`.
  pub fn size_within(&self, max: usize) -> usize {
    self.size.clamp(1, max)
  }

  /// Numeric cursor (offset / page number) with a default for the first page.
  pub fn number_or(&self, first: u64) -> u64 {
    self
      .cursor
      .as_deref()
      .and_then(|c| c.parse().ok())
      .unwrap_or(first)
  }
}

#[derive(Debug, Clone, Default)]
pub struct Query {
  pub keyword: String,
  /// One of `Choices::search_sort`.
  pub sort: Option<String>,
  /// One of `Choices::search_filter`.
  pub filter: Option<String>,
}

/// Content to publish.
#[derive(Debug, Clone, Default)]
pub struct Draft {
  pub title: Option<String>,
  pub text: String,
  pub images: Vec<PathBuf>,
  /// Post to reply to (threads / replies).
  pub reply_to: Option<String>,
  /// Post to quote / repost with comment.
  pub quote: Option<String>,
  pub topics: Vec<String>,
}

/// A comment to send. `images` stay empty unless the platform declares
/// [`Cap::CommentImages`].
#[derive(Debug, Clone, Default)]
pub struct Reply {
  pub text: String,
  /// Comment of the post this one answers.
  pub reply_to: Option<String>,
  pub images: Vec<PathBuf>,
}

/// A message to send to a chat.
#[derive(Debug, Clone, Default)]
pub struct Message {
  pub text: String,
  /// Message of the chat this one answers.
  pub reply_to: Option<String>,
  /// Files to attach (images, documents ...).
  pub files: Vec<PathBuf>,
}

/// A started QR login: `url` is encoded into the QR code, `token` identifies
/// it when polling. Platforms that hand out a QR image instead put it in
/// `image` (PNG, JPEG or GIF); it is decoded and shown the same way.
#[derive(Debug, Clone, Default)]
pub struct QrTicket {
  pub url: String,
  pub token: String,
  pub extra: BTreeMap<String, String>,
  pub image: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrStatus {
  Waiting,
  Scanned,
  /// Login finished; the platform has put the session cookies into `ctx.http`.
  Confirmed,
  Expired,
}
