//! Facebook payloads into the shared model. The Comet tree (paths after
//! tamnd/facebook-cli: the same story arrives under different nestings per
//! route, so lookups try several paths and fall back to searching the subtree
//! by a stable key) and the Graph API's objects ([`graph`]).

mod comments;
pub mod graph;
mod profile;
mod story;

use atp_core::text::from_secs;
use atp_core::{Media, MediaKind, Notification, User, Value, ValueExt};

pub use self::comments::comments;
pub use self::profile::profile;
pub use self::story::{connection, permalink, stories, timeline};
use crate::refs::{canonical, handle_of};
use crate::relay::find_key;

/// A `TextWithEntities`' text.
pub fn text(v: &Value) -> Option<String> {
  v.str("text").filter(|t| !t.trim().is_empty())
}

pub fn user(m: &Value) -> User {
  let url = m
    .first_str(&["url", "profile_url", "permalink_url"])
    .map(|u| canonical(&u));
  User {
    id: m.first_str(&["id", "user_id"]).unwrap_or_default(),
    name: m
      .first_str(&["name", "short_name", "title"])
      .unwrap_or_default(),
    handle: url.as_deref().and_then(handle_of),
    url,
    avatar: m.first_str(&["profile_picture.uri", "profilePicture.uri"]),
    verified: m.bool("is_verified").unwrap_or(false)
      || m.bool("show_verified_badge_on_profile").unwrap_or(false),
    ..User::default()
  }
}

/// The best stream of a video node.
pub fn video(v: &Value) -> Option<Media> {
  let node = find_key(v, "browser_native_hd_url")
    .or_else(|| find_key(v, "playable_url"))
    .unwrap_or(v);
  let url = node.first_str(&[
    "browser_native_hd_url",
    "playable_url_quality_hd",
    "browser_native_sd_url",
    "playable_url",
  ])?;
  let mut m = Media::new(MediaKind::Video, url);
  m.width = node.u64("width").map(|w| w as u32);
  m.height = node.u64("height").map(|h| h as u32);
  m.duration = node
    .f64("playable_duration_in_ms")
    .map(|ms| ms / 1000.0)
    .or_else(|| node.f64("length_in_second"));
  Some(m)
}

/// One entry of the notifications page (`notif` or the edge node itself).
pub fn notification(edge: &Value) -> Notification {
  let n = if edge.at("notif").is_object() {
    edge.at("notif")
  } else {
    edge
  };
  Notification {
    id: n
      .str("notif_id")
      .or_else(|| n.str("id"))
      .unwrap_or_default(),
    kind: n.str("notif_type").unwrap_or_else(|| "notification".into()),
    text: n.str("body.text").unwrap_or_default(),
    url: n.str("url").map(|u| canonical(&u)),
    created_at: n
      .i64("creation_time.timestamp")
      .or_else(|| n.i64("creation_time"))
      .and_then(from_secs),
    unread: n.str("seen_state").map(|s| s != "SEEN_AND_READ"),
    raw: Some(n.clone()),
    ..Notification::default()
  }
}

/// `28M`, `2.3K`, `1,234` as a number.
pub fn approx(s: &str) -> Option<u64> {
  let s = s.trim().replace(',', "");
  let (number, scale) = match s.chars().last()? {
    'K' => (&s[..s.len() - 1], 1e3),
    'M' => (&s[..s.len() - 1], 1e6),
    'B' => (&s[..s.len() - 1], 1e9),
    _ => (s.as_str(), 1.0),
  };
  number
    .parse::<f64>()
    .ok()
    .map(|n| (n * scale).round() as u64)
}

/// The number written just before `suffix` in a sentence.
fn count_before(text: &str, suffix: &str) -> Option<u64> {
  let at = text.find(suffix)?;
  approx(text[..at].rsplit(' ').next()?)
}
