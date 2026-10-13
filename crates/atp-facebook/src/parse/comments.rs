//! Comments a story ships with its page (`comment_rendering_instance…`).

use atp_core::text::from_secs;
use atp_core::{Comment, Metrics, Value, ValueExt};

use super::story::feedback;
use super::{text, user};
use crate::refs::canonical;
use crate::relay::{edges, find_key};

/// The comments of a story and their cursor; `author` marks the post
/// author's own comments.
pub fn comments(n: &Value, author: Option<&str>) -> (Vec<Comment>, Option<String>) {
  let conn = [
    "comment_rendering_instance_for_feed_location",
    "comment_rendering_instance",
  ]
  .iter()
  .find_map(|key| {
    find_key(n, key)
      .map(|m| m.at(&format!("{key}.comments")))
      .filter(|c| c.is_object())
  });
  let Some(conn) = conn else {
    return (Vec::new(), None);
  };
  let list = edges(conn)
    .into_iter()
    .map(|c| comment(c, author))
    .filter(|c| !c.id.is_empty())
    .collect();
  (list, conn.str("page_info.end_cursor"))
}

fn comment(n: &Value, author: Option<&str>) -> Comment {
  let mut c = Comment {
    id: n
      .str("legacy_fbid")
      .or_else(|| n.str("id"))
      .unwrap_or_default(),
    author: n.at("author").is_object().then(|| user(n.at("author"))),
    text: text(n.at("body"))
      .or_else(|| find_key(n, "body").and_then(|b| text(b.at("body"))))
      .unwrap_or_default(),
    created_at: n.i64("created_time").and_then(from_secs),
    likes: n.at("feedback").is_object().then(|| {
      let mut m = Metrics::default();
      feedback(n.at("feedback"), &mut m);
      m.likes.unwrap_or(0)
    }),
    reply_count: n.count("feedback.replies_fields.total_count"),
    raw: Some(n.clone()),
    ..Comment::default()
  };
  if let Some(url) = n.str("feedback.url").or_else(|| n.str("url")) {
    c.extra.insert("url".into(), canonical(&url).into());
  }
  if let (Some(a), Some(me)) = (&c.author, author)
    && a.id == me
  {
    c.extra.insert("by_author".into(), true.into());
  }
  c
}
