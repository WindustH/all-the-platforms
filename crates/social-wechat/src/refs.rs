//! Chats are WeChat user ids as iLink gives them (`…@im.wechat`); messages
//! are `CHAT/MESSAGE`, with the server's message id (or the client id of a
//! message this machine sent).

use social_core::{Error, Result};

pub fn chat(arg: &str) -> Result<String> {
  let chat = arg.trim();
  if chat.is_empty() || chat.contains(char::is_whitespace) {
    return Err(Error::input(format!("not a WeChat chat id: {arg:?}")));
  }
  Ok(chat.to_owned())
}

/// `(chat, message)` of a `CHAT/MESSAGE` reference.
pub fn message(arg: &str) -> Result<(String, String)> {
  match arg.trim().rsplit_once('/') {
    Some((chat, id)) if !chat.is_empty() && !id.is_empty() => Ok((chat.to_owned(), id.to_owned())),
    _ => Err(Error::input(format!(
      "not a WeChat message: {arg:?} (expected CHAT/MESSAGE, as `messages` lists them)"
    ))),
  }
}
