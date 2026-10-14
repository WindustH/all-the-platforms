//! Items and users as arguments.
//!
//! - items (stories, comments, jobs, polls): `https://news.ycombinator.com/item?id=<id>`,
//!   `item?id=<id>`, or the id;
//! - users: `https://news.ycombinator.com/user?id=<name>`, `user?id=<name>`,
//!   `@name` or the name.

use atp_core::{Error, Result};
use url::Url;

/// The `id` query value of a link or relative path (`item?id=1`).
fn id_param(arg: &str) -> Option<String> {
  let url = Url::parse(arg)
    .or_else(|_| {
      Url::parse(&format!(
        "https://news.ycombinator.com/{}",
        arg.trim_start_matches('/')
      ))
    })
    .ok()?;
  url
    .query_pairs()
    .find(|(key, _)| key == "id")
    .map(|(_, value)| value.into_owned())
}

pub fn item(arg: &str) -> Result<u64> {
  let arg = arg.trim();
  let id = if arg.contains("id=") {
    id_param(arg)
  } else {
    Some(arg.to_owned())
  };
  id.and_then(|id| id.parse().ok()).ok_or_else(|| {
    Error::input(format!(
      "not a Hacker News item: {arg} (expected an item link or id)"
    ))
  })
}

pub fn user(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let name = if arg.contains("id=") {
    id_param(arg).unwrap_or_default()
  } else {
    arg.trim_start_matches('@').to_owned()
  };
  let valid = !name.is_empty()
    && name.len() <= 32
    && name
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
  if valid {
    Ok(name)
  } else {
    Err(Error::input(format!(
      "not a Hacker News user: {arg} (expected a profile link or a name)"
    )))
  }
}
