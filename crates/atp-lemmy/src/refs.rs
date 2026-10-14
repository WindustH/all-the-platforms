//! Posts, comments, communities and users as arguments.
//!
//! - posts: `https://<instance>/post/<id>`, `post/<id>`, or the id; a post
//!   link of another instance is looked up through this one;
//! - comments: `https://<instance>/comment/<id>`, `comment/<id>`;
//! - communities: `!name@host`, `!name`, `c/name`, `name@host`, `name`, or a
//!   link `https://<host>/c/<name>`;
//! - users: `@name@host`, `@name`, `u/name`, `name`, or a link
//!   `https://<host>/u/<name>`.

use atp_core::{Error, Result};
use url::Url;

/// A post or comment: its id on this instance, or a link elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
  Post(u64),
  Comment(u64),
  /// A link of another instance, resolved with `resolve_object`.
  Remote(String),
}

/// The local path of a link on `instance` (`post/1`), the argument itself
/// when it is no link, or `None` for a link of another host.
fn local_path(arg: &str, instance: &str) -> Option<String> {
  match Url::parse(arg) {
    Ok(url) if url.host_str() == Some(instance) => Some(url.path().trim_matches('/').to_owned()),
    Ok(_) => None,
    Err(_) => Some(arg.trim_matches('/').to_owned()),
  }
}

/// A post or comment; a bare id is a post.
pub fn target(arg: &str, instance: &str) -> Result<Target> {
  let arg = arg.trim();
  let Some(path) = local_path(arg, instance) else {
    return Ok(Target::Remote(arg.to_owned()));
  };
  let (kind, id) = path.split_once('/').unwrap_or(("post", &path));
  let id: u64 = id.parse().map_err(|_| bad(arg, "post or comment"))?;
  match kind {
    "post" => Ok(Target::Post(id)),
    "comment" => Ok(Target::Comment(id)),
    _ => Err(bad(arg, "post or comment")),
  }
}

fn bad(arg: &str, what: &str) -> Error {
  Error::input(format!("not a Lemmy {what}: {arg}"))
}

/// A comment id as `comments` lists it, or a comment link.
pub fn comment_id(arg: &str, instance: &str) -> Result<u64> {
  if let Ok(id) = arg.trim().parse() {
    return Ok(id);
  }
  match target(arg, instance)? {
    Target::Comment(id) => Ok(id),
    _ => Err(bad(arg, "comment")),
  }
}

/// `name` or `name@host` as the API takes it: the host only when it is
/// not this instance.
fn qualified(name: &str, host: Option<&str>, instance: &str) -> String {
  match host {
    Some(host) if host != instance => format!("{name}@{host}"),
    _ => name.to_owned(),
  }
}

/// A name with a prefix (`!`, `@`), a path prefix (`c/`, `u/`) or a link
/// `https://host/<segment>/<name>`.
fn actor(arg: &str, sigil: char, segment: &str, instance: &str) -> Option<String> {
  let arg = arg.trim();
  if let Ok(url) = Url::parse(arg) {
    let host = url.host_str()?.to_owned();
    let path = url.path().trim_matches('/');
    let name = path.strip_prefix(&format!("{segment}/"))?;
    let (name, other) = name.split_once('@').unwrap_or((name, &host));
    return Some(qualified(name, Some(other), instance));
  }
  let name = arg
    .strip_prefix(sigil)
    .or_else(|| arg.strip_prefix(&format!("{segment}/")))
    .or_else(|| arg.strip_prefix(&format!("/{segment}/")))
    .unwrap_or(arg);
  let (name, host) = match name.split_once('@') {
    Some((name, host)) => (name, Some(host.to_ascii_lowercase())),
    None => (name, None),
  };
  let ok = !name.is_empty()
    && name
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.');
  ok.then(|| qualified(name, host.as_deref(), instance))
}

/// A community as the API's `community_name` takes it.
pub fn community(arg: &str, instance: &str) -> Result<String> {
  actor(arg, '!', "c", instance).ok_or_else(|| bad(arg, "community (!name@host)"))
}

/// A user as the API's `username` takes it.
pub fn user(arg: &str, instance: &str) -> Result<String> {
  actor(arg, '@', "u", instance).ok_or_else(|| bad(arg, "user (@name@host)"))
}
