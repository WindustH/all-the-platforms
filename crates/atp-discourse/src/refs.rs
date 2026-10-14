//! Topics, posts, users and categories as arguments.
//!
//! - topics: the id, or a link `/t/<slug>/<id>`; `/t/<slug>/<id>/<n>`
//!   points at post number `n` in it;
//! - posts: `/p/<post id>`, `post:<post id>`, or the ids `comments`
//!   lists (where a post is expected);
//! - users: `me`, `@name`, `name`, or a link `/u/<name>`;
//! - categories: the id, the slug, or a link `/c/<slug>/<id>`.

use atp_core::{Error, Result};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostRef {
  /// A topic, or the post with that number in it.
  Topic { id: u64, number: Option<u64> },
  /// A post by its own id.
  Post(u64),
}

/// The path segments of a link, or `None` for anything else.
fn segments(arg: &str) -> Option<Vec<String>> {
  let url = Url::parse(arg).ok()?;
  if !url.scheme().starts_with("http") {
    return None;
  }
  Some(url.path_segments()?.map(str::to_owned).collect())
}

fn number(text: &str) -> Option<u64> {
  text.parse().ok().filter(|n| *n > 0)
}

pub fn post(arg: &str) -> Result<PostRef> {
  let arg = arg.trim();
  if let Some(id) = number(arg) {
    return Ok(PostRef::Topic { id, number: None });
  }
  if let Some(id) = arg.strip_prefix("post:").and_then(number) {
    return Ok(PostRef::Post(id));
  }
  let bad = || {
    Error::input(format!(
      "not a topic or post: {arg} (expected an id or a /t/ link)"
    ))
  };
  let segments = segments(arg).ok_or_else(bad)?;
  let at = segments
    .iter()
    .position(|s| s == "t" || s == "p")
    .ok_or_else(bad)?;
  let rest: Vec<&str> = segments[at + 1..].iter().map(String::as_str).collect();
  if segments[at] == "p" {
    return rest
      .first()
      .and_then(|id| number(id))
      .map(PostRef::Post)
      .ok_or_else(bad);
  }
  let (id, number_part) = match rest.as_slice() {
    [id] => (number(id), None),
    [first, second, ..] if number(first).is_some() => (number(first), Some(*second)),
    [_, id] => (number(id), None),
    [_, id, n, ..] => (number(id), Some(*n)),
    _ => (None, None),
  };
  let id = id.ok_or_else(bad)?;
  Ok(PostRef::Topic {
    id,
    number: number_part.and_then(number).filter(|n| *n > 1),
  })
}

pub fn user(arg: &str, me: Option<&str>) -> Result<String> {
  let arg = arg.trim();
  if arg == "me" {
    return me
      .map(str::to_owned)
      .ok_or_else(|| Error::auth("`me` needs a login (the username is saved with it)"));
  }
  if let Some(segments) = segments(arg) {
    let at = segments.iter().position(|s| s == "u");
    return at
      .and_then(|at| segments.get(at + 1))
      .filter(|name| !name.is_empty())
      .cloned()
      .ok_or_else(|| Error::input(format!("not a user link: {arg}")));
  }
  let name = arg.trim_start_matches('@');
  if name.is_empty() || name.contains(char::is_whitespace) {
    return Err(Error::input(format!("not a user: {arg}")));
  }
  Ok(name.to_owned())
}

/// A category: its id when the argument names one, else its slug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryRef {
  Id(u64),
  Slug(String),
}

pub fn category(arg: &str) -> Result<CategoryRef> {
  let arg = arg.trim();
  if let Some(id) = number(arg) {
    return Ok(CategoryRef::Id(id));
  }
  if let Some(segments) = segments(arg) {
    let at = segments
      .iter()
      .position(|s| s == "c")
      .ok_or_else(|| Error::input(format!("not a category link: {arg}")))?;
    let rest = &segments[at + 1..];
    if let Some(id) = rest.iter().rev().find_map(|s| number(s)) {
      return Ok(CategoryRef::Id(id));
    }
    return rest
      .first()
      .map(|slug| CategoryRef::Slug(slug.clone()))
      .ok_or_else(|| Error::input(format!("not a category link: {arg}")));
  }
  let slug = arg.trim_start_matches('#');
  if slug.is_empty() || slug.contains(char::is_whitespace) {
    return Err(Error::input(format!("not a category: {arg}")));
  }
  Ok(CategoryRef::Slug(slug.to_owned()))
}
