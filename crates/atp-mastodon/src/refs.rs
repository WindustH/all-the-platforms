//! Statuses and accounts as arguments.
//!
//! - statuses: the id, or a link on the instance in use (`/@user/<id>`,
//!   `/@user@host/<id>`, `/users/user/statuses/<id>`); links to other
//!   servers are looked up through the instance (`resolve`, needs an
//!   account);
//! - accounts: `@user` (this instance), `@user@host`, a profile link, an
//!   id, or `me`.

use atp_core::{Error, Result};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusRef {
  Id(String),
  /// A link to another server, resolved through the instance.
  Remote(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountRef {
  Me,
  Id(String),
  /// `user` or `user@host`, as `accounts/lookup` takes it.
  Acct(String),
}

fn is_id(text: &str) -> bool {
  !text.is_empty() && text.len() <= 24 && text.bytes().all(|b| b.is_ascii_digit())
}

pub fn status(arg: &str, instance: &str) -> Result<StatusRef> {
  let arg = arg.trim();
  if is_id(arg) {
    return Ok(StatusRef::Id(arg.to_owned()));
  }
  let Ok(url) = Url::parse(arg) else {
    return Err(Error::input(format!(
      "not a status: {arg} (expected an id or a link)"
    )));
  };
  if url.host_str() != Some(instance) {
    return Ok(StatusRef::Remote(arg.to_owned()));
  }
  let segments: Vec<&str> = url
    .path_segments()
    .map(Iterator::collect)
    .unwrap_or_default();
  let id = match segments.as_slice() {
    [user, id, ..] if user.starts_with('@') => *id,
    ["users", _, "statuses", id, ..] => *id,
    ["deck", user, id, ..] if user.starts_with('@') => *id,
    _ => "",
  };
  if is_id(id) {
    return Ok(StatusRef::Id(id.to_owned()));
  }
  Err(Error::input(format!("not a status link: {arg}")))
}

pub fn account(arg: &str, instance: &str) -> Result<AccountRef> {
  let arg = arg.trim();
  if arg == "me" {
    return Ok(AccountRef::Me);
  }
  if is_id(arg) {
    return Ok(AccountRef::Id(arg.to_owned()));
  }
  if let Ok(url) = Url::parse(arg) {
    let host = url.host_str().unwrap_or_default().to_owned();
    let name = url
      .path_segments()
      .and_then(|mut s| s.next())
      .and_then(|first| first.strip_prefix('@'))
      .ok_or_else(|| Error::input(format!("not a profile link: {arg}")))?;
    return Ok(AccountRef::Acct(acct(name, &host, instance)));
  }
  let name = arg.strip_prefix('@').unwrap_or(arg);
  if name.is_empty() || name.contains(char::is_whitespace) {
    return Err(Error::input(format!(
      "not an account: {arg} (expected @user, @user@host, a profile link or an id)"
    )));
  }
  Ok(AccountRef::Acct(acct(name, instance, instance)))
}

/// `user` on the instance in use, else `user@host`.
fn acct(name: &str, host: &str, instance: &str) -> String {
  match name.split_once('@') {
    Some((user, server)) if server.eq_ignore_ascii_case(instance) => user.to_owned(),
    Some(_) => name.to_owned(),
    None if host.eq_ignore_ascii_case(instance) || host.is_empty() => name.to_owned(),
    None => format!("{name}@{host}"),
  }
}

/// A hashtag argument without its `#`.
pub fn tag(arg: &str) -> Result<String> {
  let tag = arg.trim().trim_start_matches('#');
  if tag.is_empty() || tag.contains(char::is_whitespace) {
    return Err(Error::input(format!("not a hashtag: {arg}")));
  }
  Ok(tag.to_owned())
}
