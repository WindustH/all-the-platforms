//! Arguments: post ids and permalinks (`SERVER/TEAM/pl/POST`), channels as
//! ids, links (`SERVER/TEAM/channels/NAME`), `TEAM/NAME`, `~name`, or a user
//! (`@name`) for the direct channel; teams as ids or names.

use social_core::{Error, Result};
use url::Url;

/// Mattermost ids: 26 lowercase letters and digits.
pub fn is_id(text: &str) -> bool {
  text.len() == 26
    && text
      .bytes()
      .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// The post id of a permalink, `CHANNEL/POST` or an id.
pub fn post_id(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if is_id(arg) {
    return Ok(arg.to_owned());
  }
  if let Ok(url) = Url::parse(arg) {
    let segs: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
    if let Some(at) = segs.iter().position(|s| *s == "pl")
      && let Some(id) = segs.get(at + 1).filter(|s| is_id(s))
    {
      return Ok((*id).to_owned());
    }
  } else if let Some((_, id)) = arg.rsplit_once('/')
    && is_id(id)
  {
    return Ok(id.to_owned());
  }
  Err(Error::input(format!(
    "not a Mattermost post: {arg} (expected a permalink …/pl/ID or a post id)"
  )))
}

pub enum ChatRef {
  Id(String),
  /// Team name and channel name.
  Path(String, String),
  /// A user, for the direct channel.
  User(String),
  /// A channel name to look for in every team.
  Name(String),
}

pub fn chat(arg: &str) -> ChatRef {
  let arg = arg.trim();
  if is_id(arg) {
    return ChatRef::Id(arg.to_owned());
  }
  if let Ok(url) = Url::parse(arg) {
    let segs: Vec<&str> = url
      .path_segments()
      .map(|s| s.filter(|s| !s.is_empty()).collect())
      .unwrap_or_default();
    // …/TEAM/channels/NAME or …/TEAM/messages/@USER
    if let Some(at) = segs
      .iter()
      .position(|s| *s == "channels" || *s == "messages")
      && at > 0
      && let Some(name) = segs.get(at + 1)
    {
      return match name.strip_prefix('@') {
        Some(user) => ChatRef::User(user.to_owned()),
        None if is_id(name) => ChatRef::Id((*name).to_owned()),
        None => ChatRef::Path(segs[at - 1].to_owned(), (*name).to_owned()),
      };
    }
  }
  if let Some(user) = arg.strip_prefix('@') {
    return ChatRef::User(user.to_owned());
  }
  if let Some((team, name)) = arg.split_once('/') {
    return ChatRef::Path(team.to_owned(), name.trim_start_matches('~').to_owned());
  }
  ChatRef::Name(arg.trim_start_matches(['~', '#']).to_owned())
}
