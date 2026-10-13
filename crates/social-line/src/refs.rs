//! Chat arguments: a user (`U…`), group (`C…`) or room (`R…`) id, several
//! users (`U…,U…`: multicast) or everyone (`all`: broadcast).

use social_core::{Error, Result};

/// What a chat argument names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRef {
  User(String),
  Group(String),
  Room(String),
  Users(Vec<String>),
  All,
}

fn is_id(id: &str, prefix: char) -> bool {
  id.len() == 33 && id.starts_with(prefix) && id[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn chat(arg: &str) -> Result<ChatRef> {
  let arg = arg.trim();
  if arg == "all" {
    return Ok(ChatRef::All);
  }
  if arg.contains(',') {
    let ids: Vec<String> = arg.split(',').map(|s| s.trim().to_owned()).collect();
    if ids.iter().all(|i| is_id(i, 'U')) {
      return Ok(ChatRef::Users(ids));
    }
    return Err(Error::input(
      "multicast takes user ids only (U…, comma-separated)",
    ));
  }
  if is_id(arg, 'U') {
    Ok(ChatRef::User(arg.to_owned()))
  } else if is_id(arg, 'C') {
    Ok(ChatRef::Group(arg.to_owned()))
  } else if is_id(arg, 'R') {
    Ok(ChatRef::Room(arg.to_owned()))
  } else {
    Err(Error::input(format!(
      "not a LINE chat: {arg} (a user U…, group C… or room R… id, `U…,U…`, or `all`)"
    )))
  }
}
