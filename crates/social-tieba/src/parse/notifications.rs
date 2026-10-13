//! Replies, @ mentions and agrees of the inbox.

use super::*;

/// A reply / @ / agree message of the inbox.
pub fn notification(message: &Value, kind: &str) -> Notification {
  let tid = message.str("thread_id").unwrap_or_default();
  let pid = message.str("post_id").unwrap_or_default();
  Notification {
    id: if pid.is_empty() {
      tid.clone()
    } else {
      pid.clone()
    },
    kind: kind.to_owned(),
    text: message.first_str(&["content", "title"]).unwrap_or_default(),
    actor: ["replyer", "user", "agree_user"]
      .iter()
      .find_map(|k| user(message.at(k))),
    target: message.first_str(&["quote_content", "title", "fname"]),
    url: (!tid.is_empty()).then(|| {
      if pid.is_empty() {
        thread_url(&tid)
      } else {
        floor_url(&tid, &pid)
      }
    }),
    created_at: secs(message, &["time"]),
    unread: message.str("unread").map(|u| u == "1"),
    raw: Some(message.clone()),
  }
}
