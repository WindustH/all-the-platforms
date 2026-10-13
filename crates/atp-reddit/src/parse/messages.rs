//! `t4` private messages, alone and as threads.

use atp_core::text::from_secs;
use atp_core::{Post, Value, ValueExt};

use crate::api::WWW;

/// One `t4` message as a post.
pub fn message(d: &Value) -> Option<Post> {
  let id = d.str("name")?;
  let mut post = Post {
    id: id.clone(),
    kind: "message".into(),
    title: d.str("subject"),
    text: d.str("body"),
    url: Some(format!(
      "{WWW}/message/messages/{}",
      id.trim_start_matches("t4_")
    )),
    author: d
      .str("author")
      .filter(|n| n != "[deleted]")
      .map(|n| super::named(&n, None)),
    created_at: d.i64("created_utc").and_then(from_secs),
    raw: Some(d.clone()),
    ..Post::default()
  };
  if let Some(to) = d.str("dest") {
    post.extra.insert("to".into(), to.into());
  }
  if d.bool("new") == Some(true) {
    post.extra.insert("unread".into(), true.into());
  }
  let thread = d.str("first_message_name").unwrap_or(id);
  post.extra.insert("chat".into(), thread.into());
  Some(post)
}

/// A thread's messages: the first one and its `replies`, oldest first.
pub fn thread(root: &Value) -> Vec<Post> {
  let mut out: Vec<Post> = message(root).into_iter().collect();
  out.extend(
    root
      .list("replies.data.children")
      .iter()
      .filter_map(|r| message(r.at("data"))),
  );
  out
}
