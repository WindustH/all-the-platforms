//! Comments and replies (`id_str`: the numeric `id` overflows JavaScript).

use atp_core::text::from_secs;
use atp_core::{Comment, User, Value, ValueExt};

pub fn comment(v: &Value) -> Option<Comment> {
  let id = v.first_str(&["id_str", "id"])?;
  let author = v.str("user_name").map(|name| User {
    id: v.str("user_id").unwrap_or_default(),
    name,
    avatar: v.str("user_profile_image_url"),
    verified: v.bool("user_verified").unwrap_or(false),
    ..User::default()
  });
  let replies = v.list("reply_list").iter().filter_map(comment).collect();
  Some(Comment {
    id,
    author,
    text: v.str("text").unwrap_or_default(),
    created_at: v.i64("create_time").and_then(from_secs),
    likes: v.count("digg_count"),
    reply_count: v.count("reply_count"),
    reply_to: v.str("reply_to_comment.user_name"),
    location: v.str("publish_loc_info").filter(|l| !l.is_empty()),
    replies,
    raw: Some(v.clone()),
    ..Comment::default()
  })
}
