//! Stories, comments and users as arguments.
//!
//! - stories: `https://lobste.rs/s/<id>[/slug]`, `s/<id>`, or the short id;
//! - comments: `https://lobste.rs/c/<id>`, `c/<id>`, or a story link
//!   ending in `#c_<id>`;
//! - users: `https://lobste.rs/~name` (or `/u/name`), `~name`, `@name`,
//!   or the name.

use atp_core::{Error, Result};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemRef {
  Story(String),
  Comment(String),
}

fn is_id(text: &str) -> bool {
  (4..=12).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The path of a link or of a relative reference, without slashes around it.
fn path_of(arg: &str) -> (String, Option<String>) {
  match Url::parse(arg) {
    Ok(url) => (
      url.path().trim_matches('/').to_owned(),
      url.fragment().map(str::to_owned),
    ),
    Err(_) => (arg.trim_matches('/').to_owned(), None),
  }
}

pub fn item(arg: &str) -> Result<ItemRef> {
  let arg = arg.trim();
  let (path, fragment) = path_of(arg);
  if let Some(id) = fragment.as_deref().and_then(|f| f.strip_prefix("c_")) {
    return Ok(ItemRef::Comment(id.to_owned()));
  }
  let mut parts = path.split('/');
  let found = match (parts.next(), parts.next()) {
    (Some("s"), Some(id)) if is_id(id) => Some(ItemRef::Story(id.to_owned())),
    (Some("c"), Some(id)) if is_id(id) => Some(ItemRef::Comment(id.to_owned())),
    (Some(id), None) if is_id(id) => Some(ItemRef::Story(id.to_owned())),
    _ => None,
  };
  found.ok_or_else(|| {
    Error::input(format!(
      "not a Lobsters story or comment: {arg} (expected a /s/ or /c/ link or a short id)"
    ))
  })
}

/// A story's short id (a comment is refused).
pub fn story(arg: &str) -> Result<String> {
  match item(arg)? {
    ItemRef::Story(id) => Ok(id),
    ItemRef::Comment(_) => Err(Error::input(format!("{arg} is a comment, not a story"))),
  }
}

pub fn user(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let (path, _) = path_of(arg);
  let name = path
    .strip_prefix("u/")
    .unwrap_or(&path)
    .trim_start_matches(['~', '@']);
  let valid = !name.is_empty()
    && name.len() <= 50
    && name
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
  if valid {
    Ok(name.to_owned())
  } else {
    Err(Error::input(format!(
      "not a Lobsters user: {arg} (expected ~name, a profile link or a name)"
    )))
  }
}
