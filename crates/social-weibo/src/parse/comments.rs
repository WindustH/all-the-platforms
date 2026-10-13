//! Comments and their reply threads.

use super::*;

pub fn comment(v: &Value) -> Option<Comment> {
  let id = v.first_str(&["idstr", "id"])?;
  let text = v
    .str("text_raw")
    .unwrap_or_else(|| strip_html(&v.str("text").unwrap_or_default()));
  let mut comment = Comment {
    author: user(v.at("user")),
    text,
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    likes: v.first_count(&["like_counts", "like_count"]),
    reply_count: v.count("total_number").filter(|n| *n > 0),
    reply_to: v.str("reply_comment.user.screen_name"),
    location: v
      .str("source")
      .map(|s| s.trim_start_matches("来自").trim().to_owned())
      .filter(|s| !s.is_empty()),
    replies: v.list("comments").iter().filter_map(comment).collect(),
    raw: Some(v.clone()),
    id,
    ..Comment::default()
  };
  if let Some(pic) = v.str("pic.large.url").or_else(|| v.str("pic.url")) {
    comment.extra.insert("image".into(), pic.into());
  }
  if v.bool("isLikedByMblogAuthor") == Some(true) {
    comment.extra.insert("liked_by_author".into(), true.into());
  }
  if let Some(floor) = v.i64("floor_number") {
    comment.extra.insert("floor".into(), floor.into());
  }
  Some(comment)
}
