//! Ids of chat messages. A message without a permalink takes `CHAT/MESSAGE`
//! as its id, so commands can take it back; chat ids may contain `/`, so
//! the message id is everything after the last one.

use crate::error::{Error, Result};

pub fn message_id(chat: &str, message: &str) -> String {
  format!("{chat}/{message}")
}

/// `(chat, message)` of a `CHAT/MESSAGE` id.
pub fn split_message_id(id: &str) -> Option<(&str, &str)> {
  id.trim()
    .rsplit_once('/')
    .filter(|(chat, message)| !chat.is_empty() && !message.is_empty())
}

/// [`split_message_id`], or an input error naming the expected form.
pub fn expect_message_id(id: &str) -> Result<(&str, &str)> {
  split_message_id(id).ok_or_else(|| {
    Error::input(format!(
      "not a message: `{id}` (expected CHAT/MESSAGE, a link, or #N of a message list)"
    ))
  })
}
