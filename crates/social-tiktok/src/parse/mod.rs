//! TikTok payloads → the shared model. Items (videos and photo posts) and
//! users come in camelCase from pages and most API lists; comments and
//! search results in snake_case. Counters are read from `statsV2` (exact,
//! as strings) before `stats` (rounded on some pages).

mod collections;
mod comments;
mod items;
mod users;

use social_core::text::from_secs;
use social_core::{
  Collection, Comment, Media, MediaKind, Metrics, Post, User, UserStats, Value, ValueExt, json,
};

use crate::refs::{user_url, video_url};

pub use self::collections::*;
pub use self::comments::*;
pub use self::items::*;
pub use self::users::*;

/// A counter from `statsV2` (exact) or `stats`.
fn stat(item: &Value, v2: &str, v1: &str, key: &str) -> Option<u64> {
  item
    .count(&format!("{v2}.{key}"))
    .or_else(|| item.count(&format!("{v1}.{key}")))
}
