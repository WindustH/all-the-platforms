//! Arguments: rooms as ids, links (`SERVER/channel/NAME`, `/group/NAME`,
//! `/direct/…`), `#name` / `name`, or `@user` for the direct room;
//! messages as ids, links with `?msg=ID`, or `ROOM/ID`.

use social_core::{Error, Result};
use url::Url;

/// Meteor ids (17 characters) and direct-room ids (two of them).
pub fn is_id(s: &str) -> bool {
  (s.len() == 17 || s.len() == 34) && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub fn message(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if let Ok(url) = Url::parse(arg) {
    if let Some((_, id)) = url.query_pairs().find(|(k, _)| k == "msg") {
      return Ok(id.into_owned());
    }
    return Err(Error::input(format!("no ?msg= in the link {arg}")));
  }
  let id = arg.rsplit('/').next().unwrap_or(arg);
  if id.len() >= 10 && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
    return Ok(id.to_owned());
  }
  Err(Error::input(format!(
    "not a Rocket.Chat message: {arg} (expected a message link with ?msg=ID or an id)"
  )))
}

pub enum Room {
  Id(String),
  Name(String),
  User(String),
}

pub fn room(arg: &str) -> Room {
  let arg = arg.trim();
  if is_id(arg) {
    return Room::Id(arg.to_owned());
  }
  if let Ok(url) = Url::parse(arg) {
    let segs: Vec<&str> = url
      .path_segments()
      .map(|s| s.filter(|s| !s.is_empty()).collect())
      .unwrap_or_default();
    if let [kind, name, ..] = segs.as_slice() {
      return match *kind {
        "direct" if is_id(name) => Room::Id((*name).to_owned()),
        "direct" => Room::User((*name).to_owned()),
        _ => Room::Name((*name).to_owned()),
      };
    }
  }
  if let Some(user) = arg.strip_prefix('@') {
    return Room::User(user.to_owned());
  }
  Room::Name(arg.trim_start_matches('#').to_owned())
}
