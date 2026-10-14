//! Topics, replies, members and nodes as arguments.
//!
//! - topics: `https://www.v2ex.com/t/<id>` (a `#reply<n>` is ignored),
//!   `t/<id>`, or the id;
//! - replies: the id `comments` lists;
//! - members: `https://www.v2ex.com/member/<name>`, `member/<name>`,
//!   `@name`, or the name;
//! - nodes: `https://www.v2ex.com/go/<name>`, `go/<name>`, or the name.

use atp_core::{Error, Result};
use url::Url;

fn path_of(arg: &str) -> String {
  match Url::parse(arg) {
    Ok(url) => url.path().trim_matches('/').to_owned(),
    Err(_) => arg
      .split('#')
      .next()
      .unwrap_or(arg)
      .trim_matches('/')
      .to_owned(),
  }
}

fn is_name(text: &str) -> bool {
  !text.is_empty()
    && text.len() <= 40
    && text
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn topic(arg: &str) -> Result<u64> {
  let path = path_of(arg.trim());
  let id = path.strip_prefix("t/").unwrap_or(&path);
  id.parse().map_err(|_| {
    Error::input(format!(
      "not a V2EX topic: {arg} (expected a /t/ link or an id)"
    ))
  })
}

/// A numeric id (replies, notifications).
pub fn id(arg: &str, what: &str) -> Result<u64> {
  arg
    .trim()
    .parse()
    .map_err(|_| Error::input(format!("not a {what} id: {arg}")))
}

pub fn member(arg: &str) -> Result<String> {
  let path = path_of(arg.trim());
  let name = path
    .strip_prefix("member/")
    .or_else(|| path.strip_prefix("u/"))
    .unwrap_or(&path)
    .trim_start_matches('@');
  if is_name(name) {
    Ok(name.to_owned())
  } else {
    Err(Error::input(format!(
      "not a V2EX member: {arg} (expected a /member/ link or a name)"
    )))
  }
}

pub fn node(arg: &str) -> Result<String> {
  let path = path_of(arg.trim());
  let name = path.strip_prefix("go/").unwrap_or(&path);
  if is_name(name) {
    Ok(name.to_owned())
  } else {
    Err(Error::input(format!(
      "not a V2EX node: {arg} (expected a /go/ link or a name)"
    )))
  }
}
