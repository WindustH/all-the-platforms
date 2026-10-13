//! Feishu ids are typed by prefix: chats `oc_`, messages `om_`, threads
//! `omt_`, users `ou_` (open id) / `on_` (union id); anything else names a
//! user by user id, or by email when it has an `@`.

use atp_core::{Error, Result};

/// Where a message goes: `(receive_id_type, receive_id)`.
pub fn receiver(arg: &str) -> (&'static str, String) {
  let arg = arg.trim();
  let kind = if arg.starts_with("oc_") {
    "chat_id"
  } else if arg.starts_with("ou_") {
    "open_id"
  } else if arg.starts_with("on_") {
    "union_id"
  } else if arg.contains('@') {
    "email"
  } else {
    "user_id"
  };
  (kind, arg.to_owned())
}

/// `(user_id_type, id)` of a user argument.
pub fn user(arg: &str) -> (&'static str, String) {
  let arg = arg.trim().trim_start_matches('@');
  let kind = if arg.starts_with("ou_") {
    "open_id"
  } else if arg.starts_with("on_") {
    "union_id"
  } else {
    "user_id"
  };
  (kind, arg.to_owned())
}

/// A message id (`om_…`), also from `CHAT/om_…`.
pub fn message(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let id = arg.rsplit('/').next().unwrap_or(arg);
  if id.starts_with("om_") {
    Ok(id.to_owned())
  } else {
    Err(Error::input(format!(
      "not a Feishu message: {arg}; message ids start with `om_`"
    )))
  }
}

/// `(container_id_type, id)` of a chat or thread whose messages to list.
pub fn container(arg: &str) -> Result<(&'static str, String)> {
  let arg = arg.trim();
  if arg.starts_with("oc_") {
    Ok(("chat", arg.to_owned()))
  } else if arg.starts_with("omt_") {
    Ok(("thread", arg.to_owned()))
  } else {
    Err(Error::input(format!(
      "not a Feishu chat: {arg}; chat ids start with `oc_`, threads with `omt_` (see `atp feishu chats`)"
    )))
  }
}

pub fn chat(arg: &str) -> Result<String> {
  match container(arg)? {
    ("chat", id) => Ok(id),
    _ => Err(Error::input(format!(
      "not a chat: {arg} (chat ids start with `oc_`)"
    ))),
  }
}
