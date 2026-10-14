//! Posts and users as arguments.
//!
//! - posts: the id (digits), or a link `https://www.threads.com/@user/post/<code>`
//!   (threads.net too), or `https://www.threads.com/t/<code>`;
//! - users: `me`, `@username`, `username`, or a profile link.

use atp_core::{Error, Result};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostRef {
  Id(String),
  /// A link: the shortcode, and the author when the link names one.
  Code {
    user: Option<String>,
    code: String,
  },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserRef {
  Me,
  Name(String),
}

fn is_threads(url: &Url) -> bool {
  url.host_str().is_some_and(|host| {
    let host = host.trim_start_matches("www.");
    host == "threads.com" || host == "threads.net"
  })
}

pub fn post(arg: &str) -> Result<PostRef> {
  let arg = arg.trim();
  if !arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(PostRef::Id(arg.to_owned()));
  }
  let url = Url::parse(arg).ok().filter(is_threads).ok_or_else(|| {
    Error::input(format!(
      "not a post: {arg} (expected an id or a threads.com link)"
    ))
  })?;
  let segments: Vec<&str> = url
    .path_segments()
    .map(Iterator::collect)
    .unwrap_or_default();
  match segments.as_slice() {
    [user, "post", code, ..] if user.starts_with('@') && !code.is_empty() => Ok(PostRef::Code {
      user: Some(user.trim_start_matches('@').to_ascii_lowercase()),
      code: code.to_string(),
    }),
    ["t", code, ..] if !code.is_empty() => Ok(PostRef::Code {
      user: None,
      code: code.to_string(),
    }),
    _ => Err(Error::input(format!("not a post link: {arg}"))),
  }
}

pub fn user(arg: &str) -> Result<UserRef> {
  let arg = arg.trim();
  if arg == "me" {
    return Ok(UserRef::Me);
  }
  if let Ok(url) = Url::parse(arg) {
    let name = Some(&url)
      .filter(|url| is_threads(url))
      .and_then(|url| url.path_segments()?.next().map(str::to_owned))
      .and_then(|first| first.strip_prefix('@').map(str::to_owned))
      .ok_or_else(|| Error::input(format!("not a profile link: {arg}")))?;
    return Ok(UserRef::Name(name.to_ascii_lowercase()));
  }
  let name = arg.trim_start_matches('@');
  let valid = !name.is_empty()
    && name
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
  if !valid {
    return Err(Error::input(format!(
      "not a user: {arg} (expected me, @username or a profile link)"
    )));
  }
  Ok(UserRef::Name(name.to_ascii_lowercase()))
}

pub fn profile_url(username: &str) -> String {
  format!("https://www.threads.com/@{username}")
}
