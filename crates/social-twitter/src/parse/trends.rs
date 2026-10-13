//! Trends of the Explore tabs, and Spaces, as posts.

use social_core::text::from_millis;
use social_core::{Post, Value, ValueExt};

use crate::refs::space_url;

pub fn trend(item: &Value) -> Option<Post> {
  if item.str("__typename").as_deref() != Some("TimelineTrend") {
    return None;
  }
  let name = item.str("name")?;
  let context: Vec<String> = [
    "trend_metadata.domain_context",
    "trend_metadata.meta_description",
  ]
  .iter()
  .filter_map(|p| item.str(p))
  .collect();
  let search = url::Url::parse_with_params("https://x.com/search", &[("q", name.as_str())])
    .map(String::from)
    .ok();
  let mut post = Post {
    kind: "trend".into(),
    title: Some(name.clone()),
    text: (!context.is_empty()).then(|| context.join(" · ")),
    url: search,
    tags: item
      .list("grouped_trends")
      .iter()
      .filter_map(|g| g.str("name"))
      .collect(),
    raw: Some(item.clone()),
    id: name,
    ..Post::default()
  };
  if let Some(rank) = item.u64("rank") {
    post.extra.insert("rank".into(), rank.into());
  }
  Some(post)
}

/// An `audioSpace` object: `metadata` plus `participants`.
pub fn space(space: &Value) -> Option<Post> {
  let m = space.at("metadata");
  let id = m.str("rest_id")?;
  let time = |k: &str| m.i64(k).and_then(from_millis);
  let names = |role: &str| -> Vec<Value> {
    space
      .list(&format!("participants.{role}"))
      .iter()
      .filter_map(|p| p.str("twitter_screen_name").map(Value::from))
      .collect()
  };
  let mut post = Post {
    kind: "space".into(),
    title: m.str("title"),
    url: Some(space_url(&id)),
    author: super::user(m.at("creator_results.result")),
    created_at: time("started_at").or_else(|| time("created_at")),
    raw: Some(space.clone()),
    id,
    ..Post::default()
  };
  post.metrics.views = m
    .count("total_live_listeners")
    .or_else(|| m.count("total_replay_watched"));
  let mut put = |key: &str, value: Option<Value>| {
    if let Some(value) = value {
      post.extra.insert(key.into(), value);
    }
  };
  put("state", m.str("state").map(Value::from));
  for key in ["scheduled_start", "ended_at", "updated_at"] {
    put(key, time(key).map(|t| Value::from(t.to_string())));
  }
  put(
    "live_listeners",
    m.count("total_live_listeners").map(Value::from),
  );
  put(
    "replay_views",
    m.count("total_replay_watched").map(Value::from),
  );
  put(
    "replay",
    m.bool("is_space_available_for_replay").map(Value::from),
  );
  put(
    "participants",
    space.count("participants.total").map(Value::from),
  );
  put("media_key", m.str("media_key").map(Value::from));
  for role in ["admins", "speakers"] {
    let list = names(role);
    put(role, (!list.is_empty()).then(|| list.into()));
  }
  Some(post)
}
