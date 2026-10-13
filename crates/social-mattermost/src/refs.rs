//! Arguments: post ids and permalinks (`SERVER/TEAM/pl/POST`), channels as
//! ids, links (`SERVER/TEAM/channels/NAME`), `TEAM/NAME`, `~name`, or a user
//! (`@name`) for the direct channel; teams as ids or names.

use social_core::{Error, Result};
use url::Url;

/// Mattermost ids: 26 lowercase letters and digits.
pub fn is_id(s: &str) -> bool {
  s.len() == 26
    && s
      .bytes()
      .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// The post id of a permalink, `CHANNEL/POST` or an id.
pub fn post(arg: &str) -> Result<String> {
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

pub enum Chat {
  Id(String),
  /// Team name and channel name.
  Path(String, String),
  /// A user, for the direct channel.
  User(String),
  /// A channel name to look for in every team.
  Name(String),
}

pub fn chat(arg: &str) -> Chat {
  let arg = arg.trim();
  if is_id(arg) {
    return Chat::Id(arg.to_owned());
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
        Some(user) => Chat::User(user.to_owned()),
        None if is_id(name) => Chat::Id((*name).to_owned()),
        None => Chat::Path(segs[at - 1].to_owned(), (*name).to_owned()),
      };
    }
  }
  if let Some(user) = arg.strip_prefix('@') {
    return Chat::User(user.to_owned());
  }
  if let Some((team, name)) = arg.split_once('/') {
    return Chat::Path(team.to_owned(), name.trim_start_matches('~').to_owned());
  }
  Chat::Name(arg.trim_start_matches(['~', '#']).to_owned())
}
