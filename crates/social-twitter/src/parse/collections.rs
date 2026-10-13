//! Lists and communities as collections.

use social_core::text::from_millis;
use social_core::{Collection, Value, ValueExt};

use crate::refs::{community_url, list_url};

pub fn list(v: &Value) -> Option<Collection> {
  let id = v.str("id_str")?;
  let mut c = Collection {
    kind: "list".into(),
    name: v.str("name").unwrap_or_default(),
    description: v.str("description"),
    url: Some(list_url(&id)),
    items: v.count("member_count"),
    followers: v.count("subscriber_count"),
    owner: super::user(v.at("user_results.result")),
    raw: Some(v.clone()),
    id,
    ..Collection::default()
  };
  if let Some(mode) = v.str("mode") {
    c.extra.insert("mode".into(), mode.to_lowercase().into());
  }
  Some(c)
}

pub fn community(v: &Value) -> Option<Collection> {
  let id = v.str("rest_id").or_else(|| v.str("id_str"))?;
  let mut c = Collection {
    kind: "community".into(),
    name: v.str("name").unwrap_or_default(),
    description: v.str("description"),
    url: Some(community_url(&id)),
    followers: v.count("member_count"),
    owner: super::user(v.at("creator_results.result"))
      .or_else(|| super::user(v.at("admin_results.result"))),
    raw: Some(v.clone()),
    id,
    ..Collection::default()
  };
  let mut put = |key: &str, value: Option<Value>| {
    if let Some(value) = value {
      c.extra.insert(key.into(), value);
    }
  };
  put("moderators", v.count("moderator_count").map(Value::from));
  put("member", v.bool("is_member").map(Value::from));
  put("role", v.str("role").map(Value::from));
  put("join_policy", v.str("join_policy").map(Value::from));
  put("nsfw", v.bool("is_nsfw").map(Value::from));
  put("question", v.str("question").map(Value::from));
  put(
    "created_at",
    v.i64("created_at")
      .and_then(from_millis)
      .map(|t| Value::from(t.to_string())),
  );
  let rules: Vec<Value> = v
    .list("rules")
    .iter()
    .filter_map(|r| r.str("name").map(Value::from))
    .collect();
  if !rules.is_empty() {
    put("rules", Some(rules.into()));
  }
  put(
    "banner",
    v.str("default_banner_media.media_info.original_img_url")
      .or_else(|| v.str("custom_banner_media.media_info.original_img_url"))
      .map(Value::from),
  );
  Some(c)
}

/// Every `Community` object in a response (the slice's shape varies).
pub fn communities(v: &Value, out: &mut Vec<Collection>) {
  match v {
    Value::Object(m) => {
      if v.str("__typename").as_deref() == Some("Community")
        && let Some(c) = community(v)
      {
        if !out.iter().any(|o| o.id == c.id) {
          out.push(c);
        }
        return;
      }
      for child in m.values() {
        communities(child, out);
      }
    }
    Value::Array(a) => a.iter().for_each(|c| communities(c, out)),
    _ => {}
  }
}
