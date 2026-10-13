//! Watch pages: `player` (details, microformat) plus `next` (owner, likes,
//! chapters, the heatmap).

use jiff::Timestamp;
use social_core::{Extra, Metrics, Post, User, Value, ValueExt, json};

use super::video::{base, thumbnail};
use super::{count, find, first, image, number_in, put, text, verified, when};
use crate::refs::channel_url;

/// Chapters of a watch page: `[{title, start}]` and whether YouTube made them.
fn chapters(next: &Value) -> Option<(Value, bool)> {
  let markers = next.list("playerOverlays.playerOverlayRenderer.decoratedPlayerBarRenderer.decoratedPlayerBarRenderer.playerBar.multiMarkersPlayerBarRenderer.markersMap");
  let map = markers
    .iter()
    .find(|m| m.str("key").is_some_and(|k| k.ends_with("CHAPTERS")))?;
  let list: Vec<Value> = find(map, "chapterRenderer")
    .iter()
    .map(|c| {
      json!({
        "title": text(c.at("title")),
        "start": c.u64("timeRangeStartMillis").map(|ms| ms as f64 / 1000.0),
      })
    })
    .collect();
  let auto = map.str("key").as_deref() == Some("AUTO_CHAPTERS");
  (!list.is_empty()).then(|| (Value::from(list), auto))
}

/// The audience-retention heatmap of a watch page (`macroMarkersListEntity`
/// of type `MARKER_TYPE_HEATMAP`, as YouTube.js reads it): equal segments
/// with an intensity between 0 and 1, and the "Most replayed" ranges.
fn heatmap(next: &Value) -> Option<(Value, Value)> {
  let list = find(next, "macroMarkersListEntity")
    .into_iter()
    .map(|e| e.at("markersList"))
    .find(|l| l.str("markerType").as_deref() == Some("MARKER_TYPE_HEATMAP"))?;
  let markers = list.list("markers");
  let segment = markers.first()?.f64("durationMillis")? / 1000.0;
  let intensity: Vec<Value> = markers
    .iter()
    .map(|m| {
      let i = m.f64("intensityScoreNormalized").unwrap_or(0.0);
      json!((i * 1000.0).round() / 1000.0)
    })
    .collect();
  let replayed: Vec<Value> = list
    .list("markersDecoration.timedMarkerDecorations")
    .iter()
    .filter_map(|d| {
      Some(json!({
        "start": d.f64("visibleTimeRangeStartMillis")? / 1000.0,
        "end": d.f64("visibleTimeRangeEndMillis")? / 1000.0,
        "peak": d.f64("decorationTimeMillis").map(|ms| ms / 1000.0),
      }))
    })
    .collect();
  Some((
    json!({ "segment": segment, "intensity": intensity }),
    Value::from(replayed),
  ))
}

/// A watch page: `player` (details, microformat) plus `next` (owner, likes,
/// chapters). Comments are counted separately.
pub fn full(player: &Value, next: &Value) -> Option<Post> {
  let details = player.at("videoDetails");
  let micro = player.at("microformat.playerMicroformatRenderer");
  let id = details
    .str("videoId")
    .or_else(|| micro.str("externalVideoId"))?;
  let results = next.at("contents.twoColumnWatchNextResults.results.results");
  let primary = first(results, "videoPrimaryInfoRenderer").unwrap_or(&Value::Null);
  let secondary = first(results, "videoSecondaryInfoRenderer").unwrap_or(&Value::Null);
  let short = micro.bool("isShortsEligible") == Some(true);
  let mut p = base(id, short);
  p.title = details.str("title").or_else(|| text(micro.at("title")));
  p.text = details
    .str("shortDescription")
    .or_else(|| text(micro.at("description")));
  p.author = watch_owner(details, micro, secondary.at("owner.videoOwnerRenderer"));
  p.created_at = micro
    .str("publishDate")
    .or_else(|| micro.str("uploadDate"))
    .and_then(|s| s.parse::<Timestamp>().ok())
    .or_else(|| text(primary.at("dateText")).and_then(|t| when(&t)));
  p.metrics = watch_metrics(details, micro, primary);
  p.tags = details
    .list("keywords")
    .iter()
    .filter_map(|k| k.str(""))
    .collect();
  thumbnail(&mut p, details.at("thumbnail"));
  watch_extra(&mut p.extra, details, micro, primary, next);
  watch_flags(&mut p.extra, details, micro, primary);
  p.raw = Some(json!({ "player": player, "next": next }));
  Some(p)
}

/// The channel of a watch page; `owner` is its `videoOwnerRenderer`.
fn watch_owner(details: &Value, micro: &Value, owner: &Value) -> Option<User> {
  let channel = details
    .str("channelId")
    .or_else(|| micro.str("externalChannelId"))?;
  let handle = micro
    .str("ownerProfileUrl")
    .and_then(|u| u.rsplit('/').next().map(str::to_owned))
    .filter(|h| h.starts_with('@'))
    .map(|h| h.trim_start_matches('@').to_owned());
  let mut user = User {
    name: details
      .str("author")
      .or_else(|| micro.str("ownerChannelName"))
      .unwrap_or_else(|| channel.clone()),
    url: Some(channel_url(&channel)),
    handle,
    avatar: image(owner.at("thumbnail")),
    verified: verified(owner.at("badges")),
    id: channel,
    ..User::default()
  };
  user.stats.followers = text(owner.at("subscriberCountText")).and_then(|t| count(&t));
  Some(user)
}

/// Views, and likes from the microformat or the like button's label.
fn watch_metrics(details: &Value, micro: &Value, primary: &Value) -> Metrics {
  let like_text = find(primary, "buttonViewModel")
    .iter()
    .find(|b| b.str("iconName").as_deref() == Some("LIKE"))
    .and_then(|b| b.str("accessibilityText"));
  Metrics {
    views: details.u64("viewCount").or_else(|| micro.u64("viewCount")),
    likes: micro
      .u64("likeCount")
      .or_else(|| like_text.and_then(|t| number_in(&t))),
    ..Metrics::default()
  }
}

/// Duration, category, hashtags, the heatmap and chapters.
fn watch_extra(x: &mut Extra, details: &Value, micro: &Value, primary: &Value, next: &Value) {
  put(x, "duration", details.u64("lengthSeconds"));
  put(x, "category", micro.str("category"));
  put(x, "published", text(primary.at("dateText")));
  let hashtags: Vec<String> = primary
    .list("superTitleLink.runs")
    .iter()
    .filter_map(|r| r.str("text"))
    .filter(|t| t.starts_with('#'))
    .collect();
  if !hashtags.is_empty() {
    put(x, "hashtags", hashtags);
  }
  if let Some((map, replayed)) = heatmap(next) {
    put(x, "heatmap", map);
    if replayed.as_array().is_some_and(|r| !r.is_empty()) {
      put(x, "most_replayed", replayed);
    }
  }
  if let Some((list, auto)) = chapters(next) {
    put(x, "chapters", list);
    if auto {
      put(x, "chapters_auto", true);
    }
  }
}

/// Live, upcoming, private and unlisted states, and your own rating.
fn watch_flags(x: &mut Extra, details: &Value, micro: &Value, primary: &Value) {
  for (key, flag) in [
    ("live", details.bool("isLive")),
    ("live_content", details.bool("isLiveContent")),
    ("upcoming", details.bool("isUpcoming")),
    ("private", details.bool("isPrivate")),
    ("unlisted", micro.bool("isUnlisted")),
  ] {
    if flag == Some(true) {
      put(x, key, true);
    }
  }
  put(x, "family_safe", micro.bool("isFamilySafe"));
  if let Some(status) = first(primary, "likeStatusEntity").and_then(|l| l.str("likeStatus")) {
    match status.as_str() {
      "LIKE" => put(x, "liked", true),
      "DISLIKE" => put(x, "disliked", true),
      _ => {}
    }
  }
}
