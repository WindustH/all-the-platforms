//! Comments and replies.

use super::*;

pub fn comment(v: &Value) -> Comment {
  let mut extra = atp_core::Extra::new();
  if let Some(true) = v.bool("is_author_digged") {
    extra.insert("liked_by_author".into(), true.into());
  }
  if v.i64("stick_position").is_some_and(|n| n > 0) {
    extra.insert("pinned".into(), true.into());
  }
  if let Some(id) = v.str("aweme_id") {
    extra.insert("post_id".into(), id.into());
  }
  if let Some(label) = v.str("label_text") {
    extra.insert("label".into(), label.into());
  }
  let images: Vec<Value> = v
    .list("image_list")
    .iter()
    .filter_map(|i| i.str("origin_url.url_list.0").map(Value::from))
    .collect();
  if !images.is_empty() {
    extra.insert("images".into(), images.into());
  }
  Comment {
    id: v.str("cid").unwrap_or_default(),
    author: Some(user_brief(v.at("user"))).filter(|u| !u.id.is_empty() || !u.name.is_empty()),
    text: v.str("text").unwrap_or_default(),
    created_at: v.i64("create_time").and_then(from_secs),
    likes: v.count("digg_count"),
    reply_count: v.count("reply_comment_total"),
    reply_to: v.str("reply_to_username").filter(|s| !s.is_empty()),
    location: None,
    replies: Vec::new(),
    extra,
    raw: Some(v.clone()),
    ..Comment::default()
  }
}
