//! Posts, users, topics and comments as arguments.
//!
//! - Posts: web links (`https://web.okjike.com/u/<user>/post/<id>`,
//!   `…/repost/<id>`, the old `/originalPost/<id>` and `/repost/<id>`),
//!   share links (`https://m.okjike.com/originalPosts/<id>`, `/reposts/<id>`),
//!   app links (`jike://page.jk/originalPost/<id>`), `post:<id>`,
//!   `repost:<id>`, or a bare id (24 hex digits; tried as a post, then as
//!   a repost).
//! - Users: `https://web.okjike.com/u/<username>`, `…/users/<username>`,
//!   `jike://page.jk/user/<username>`, `@username` or the username.
//! - Topics (圈子): `https://web.okjike.com/topic/<id>`, `…/topics/<id>`,
//!   `jike://page.jk/topic/<id>`, `topic:<id>` or the id.

use atp_core::{Error, Result};
use url::Url;

use crate::api::WEB;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  Post,
  Repost,
}

impl Kind {
  /// The type name the API takes (`targetType`).
  pub fn api_type(self) -> &'static str {
    match self {
      Kind::Post => "ORIGINAL_POST",
      Kind::Repost => "REPOST",
    }
  }

  /// The API's path segment (`/1.0/<segment>/get`).
  pub fn segment(self) -> &'static str {
    match self {
      Kind::Post => "originalPosts",
      Kind::Repost => "reposts",
    }
  }

  /// The web link's segment (`/u/<user>/<segment>/<id>`).
  pub fn web(self) -> &'static str {
    match self {
      Kind::Post => "post",
      Kind::Repost => "repost",
    }
  }

  pub fn from_type(kind: &str) -> Option<Self> {
    match kind {
      "ORIGINAL_POST" | "PERSONAL_UPDATE" => Some(Kind::Post),
      "REPOST" => Some(Kind::Repost),
      _ => None,
    }
  }

  fn from_segment(segment: &str) -> Option<Self> {
    match segment {
      "post" | "originalPost" | "originalPosts" => Some(Kind::Post),
      "repost" | "reposts" => Some(Kind::Repost),
      _ => None,
    }
  }
}

/// A post; `kind` is `None` for a bare id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostRef {
  pub kind: Option<Kind>,
  pub id: String,
  /// A comment the link points at (`…/post/<id>/<comment>`).
  pub comment: Option<String>,
}

pub fn is_id(text: &str) -> bool {
  text.len() == 24 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn segments(url: &Url) -> Vec<String> {
  let host = url.host_str().unwrap_or_default().to_owned();
  let mut parts: Vec<String> = url
    .path_segments()
    .map(|s| s.filter(|p| !p.is_empty()).map(str::to_owned).collect())
    .unwrap_or_default();
  // `jike://page.jk/originalPost/<id>`: the "host" is the app's page.
  if url.scheme() == "jike" {
    parts.retain(|p| p != "page.jk");
  } else if !(host == "okjike.com" || host.ends_with(".okjike.com")) {
    return Vec::new();
  }
  parts
}

fn parse_url(arg: &str) -> Option<Url> {
  Url::parse(arg)
    .ok()
    .filter(|u| matches!(u.scheme(), "http" | "https" | "jike"))
}

pub fn post(arg: &str) -> Result<PostRef> {
  let arg = arg.trim();
  let invalid = || Error::input(format!("not a Jike post: {arg}"));
  let bare = |kind, id: &str| {
    is_id(id).then(|| PostRef {
      kind,
      id: id.to_owned(),
      comment: None,
    })
  };
  if let Some(found) = bare(None, arg) {
    return Ok(found);
  }
  if let Some((prefix, id)) = arg.split_once(':')
    && let Some(kind) = Kind::from_segment(prefix)
  {
    return bare(Some(kind), id.trim()).ok_or_else(invalid);
  }
  let url = parse_url(arg).ok_or_else(invalid)?;
  let parts = segments(&url);
  // `/u/<user>/<post|repost>/<id>[/<comment>]` or `/<kind>/<id>`.
  let start = if parts.first().map(String::as_str) == Some("u") {
    2
  } else {
    0
  };
  let (Some(segment), Some(id)) = (parts.get(start), parts.get(start + 1)) else {
    return Err(invalid());
  };
  let kind = Kind::from_segment(segment).ok_or_else(invalid)?;
  let mut found = bare(Some(kind), id).ok_or_else(invalid)?;
  found.comment = parts.get(start + 2).filter(|c| is_id(c)).cloned();
  Ok(found)
}

pub fn user(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let invalid = || Error::input(format!("not a Jike user: {arg}"));
  if let Some(url) = parse_url(arg) {
    let parts = segments(&url);
    return match parts.as_slice() {
      [kind, name, ..] if matches!(kind.as_str(), "u" | "users" | "user") => Ok(name.clone()),
      _ => Err(invalid()),
    };
  }
  let name = arg.strip_prefix('@').unwrap_or(arg);
  if name.is_empty() || name.contains(['/', ' ', '?', '#']) {
    return Err(invalid());
  }
  Ok(name.to_owned())
}

pub fn topic(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let invalid = || Error::input(format!("not a Jike topic: {arg}"));
  let id = if let Some(url) = parse_url(arg) {
    match segments(&url).as_slice() {
      [kind, id, ..] if matches!(kind.as_str(), "topic" | "topics") => id.clone(),
      _ => return Err(invalid()),
    }
  } else {
    arg.strip_prefix("topic:").unwrap_or(arg).trim().to_owned()
  };
  if is_id(&id) { Ok(id) } else { Err(invalid()) }
}

pub fn post_url(username: &str, kind: Kind, id: &str) -> String {
  format!("{WEB}/u/{username}/{}/{id}", kind.web())
}

pub fn user_url(username: &str) -> String {
  format!("{WEB}/u/{username}")
}

pub fn topic_url(id: &str) -> String {
  format!("{WEB}/topic/{id}")
}
