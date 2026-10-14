//! Pages of a listing, and what paging needs of their items.

use jiff::Timestamp;
use serde::Serialize;

use super::*;

/// One page of a listing. Pass `next_cursor` back with `--cursor` to continue.
#[derive(Debug, Clone, Serialize)]
pub struct Page<T> {
  pub items: Vec<T>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub next_cursor: Option<String>,
  pub has_more: bool,
}

impl<T> Page<T> {
  pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
    let has_more = next_cursor.is_some();
    Self {
      items,
      next_cursor,
      has_more,
    }
  }

  /// A page with nothing after it.
  pub fn last(items: Vec<T>) -> Self {
    Self {
      items,
      next_cursor: None,
      has_more: false,
    }
  }
}

/// Items with a stable id, so pages an upstream repeats can be de-duplicated.
pub trait Keyed {
  fn key(&self) -> &str;
}

macro_rules! keyed {
  ($($t:ty),*) => {$(
    impl Keyed for $t {
      fn key(&self) -> &str {
        &self.id
      }
    }
  )*};
}

keyed!(Post, User, Comment, Collection, Notification, FileEntry);

/// Items with a publication time, which `--since` / `--until` filter on.
pub trait Dated {
  fn date(&self) -> Option<Timestamp>;
}

impl Dated for Post {
  fn date(&self) -> Option<Timestamp> {
    self.created_at
  }
}

impl Dated for Comment {
  fn date(&self) -> Option<Timestamp> {
    self.created_at
  }
}

impl Dated for Notification {
  fn date(&self) -> Option<Timestamp> {
    self.created_at
  }
}
