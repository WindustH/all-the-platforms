//! Chats and messages on the command line.
//!
//! - Chats: a JID (`8613800000000@s.whatsapp.net`, `…@g.us`), a phone number
//!   (`+86 138 0000 0000`), `me`, or a name the archive knows.
//! - Messages: `CHAT/MESSAGE_ID`, as `messages` prints them.

use social_core::{Error, Result};

use crate::archive::Archive;

/// A chat's JID.
pub fn chat(arg: &str, archive: &Archive, me: &str) -> Result<String> {
  let arg = arg.trim();
  if arg.contains('@') {
    return Ok(arg.to_owned());
  }
  if matches!(arg, "me" | "self") && !me.is_empty() {
    return Ok(me.to_owned());
  }
  if let Some(number) = phone(arg) {
    return Ok(format!("{number}@s.whatsapp.net"));
  }
  match archive.find(arg).as_slice() {
    [one] => Ok(one.clone()),
    [] => Err(
      Error::not_found(format!("no chat named `{arg}`")).with_hint(
        "give a phone number with its country code, a JID, or `#N` after `social whatsapp chats`",
      ),
    ),
    many => Err(Error::input(format!(
      "`{arg}` matches several chats: {}",
      many.join(", ")
    ))),
  }
}

/// Digits of a phone number written with `+`, spaces, dashes or brackets.
pub fn phone(arg: &str) -> Option<String> {
  let digits: String = arg.chars().filter(char::is_ascii_digit).collect();
  let only_number_chars = arg
    .chars()
    .all(|c| c.is_ascii_digit() || matches!(c, '+' | ' ' | '-' | '(' | ')' | '.'));
  (only_number_chars && (7..=15).contains(&digits.len())).then_some(digits)
}

/// `(chat, message id)` of a `CHAT/ID` reference.
pub fn message(arg: &str) -> Result<(String, String)> {
  match arg.trim().rsplit_once('/') {
    Some((chat, id)) if chat.contains('@') && !id.is_empty() => {
      Ok((chat.to_owned(), id.to_owned()))
    }
    _ => Err(Error::input(format!(
      "not a message reference: {arg} (expected CHAT/ID as `messages` prints it, or `#N`)"
    ))),
  }
}
