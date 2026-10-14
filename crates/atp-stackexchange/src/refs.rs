//! Questions, answers, users and sites as arguments.
//!
//! - posts: a question or answer link of any site (`/questions/<id>/…`,
//!   `/questions/<q>/<slug>/<answer>`, `…#<answer>`, `/q/<id>`, `/a/<id>`),
//!   `q/<id>`, `a/<id>`, or a bare id (a question, else an answer);
//! - comments: an id, or a link ending in `#comment<id>_<post>`;
//! - users: a profile link (`/users/<id>/…`), `users/<id>`, an id, or `me`.
//!
//! A link names its site (`superuser.com`, `math.stackexchange.com`), which
//! then wins over the default one.

use atp_core::{Error, Result};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  Question,
  Answer,
  /// A bare id: a question, else an answer.
  Any,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostRef {
  pub site: Option<String>,
  pub kind: Kind,
  pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRef {
  pub site: Option<String>,
  /// `None` for `me`.
  pub id: Option<String>,
}

fn is_id(text: &str) -> bool {
  !text.is_empty() && text.len() <= 12 && text.bytes().all(|b| b.is_ascii_digit())
}

/// The site of a link: its host without `www.`.
fn site_of(url: &Url) -> Option<String> {
  let host = url.host_str()?.to_ascii_lowercase();
  Some(host.strip_prefix("www.").unwrap_or(&host).to_owned())
}

pub fn post(arg: &str) -> Result<PostRef> {
  let arg = arg.trim();
  let bad = || {
    Error::input(format!(
      "not a question or answer: {arg} (expected a link, q/<id>, a/<id> or an id)"
    ))
  };
  if let Ok(url) = Url::parse(arg) {
    let segments: Vec<&str> = url
      .path_segments()
      .map(Iterator::collect)
      .unwrap_or_default();
    let (kind, id) = post_path(&segments, url.fragment()).ok_or_else(bad)?;
    return Ok(PostRef {
      site: site_of(&url),
      kind,
      id,
    });
  }
  let segments: Vec<&str> = arg.trim_matches('/').split('/').collect();
  let (kind, id) = match segments.as_slice() {
    [id] if is_id(id) => (Kind::Any, (*id).to_owned()),
    _ => post_path(&segments, None).ok_or_else(bad)?,
  };
  Ok(PostRef {
    site: None,
    kind,
    id,
  })
}

/// `questions/<q>[/<slug>[/<answer>]]` (an answer also as `#<answer>`),
/// `q/<id>`, `a/<id>`, `posts/<id>`.
fn post_path(segments: &[&str], fragment: Option<&str>) -> Option<(Kind, String)> {
  let numeric = |s: Option<&&str>| s.filter(|s| is_id(s)).map(|s| (*s).to_owned());
  match segments.first().copied()? {
    "questions" => {
      let question = numeric(segments.get(1))?;
      let answer =
        numeric(segments.get(3)).or_else(|| fragment.filter(|f| is_id(f)).map(str::to_owned));
      Some(match answer {
        Some(answer) => (Kind::Answer, answer),
        None => (Kind::Question, question),
      })
    }
    "q" => Some((Kind::Question, numeric(segments.get(1))?)),
    "a" => Some((Kind::Answer, numeric(segments.get(1))?)),
    "posts" => Some((Kind::Any, numeric(segments.get(1))?)),
    _ => None,
  }
}

pub fn user(arg: &str) -> Result<UserRef> {
  let arg = arg.trim();
  if arg == "me" {
    return Ok(UserRef {
      site: None,
      id: None,
    });
  }
  let bad = || {
    Error::input(format!(
      "not a user: {arg} (expected a profile link, an id or `me`)"
    ))
  };
  if let Ok(url) = Url::parse(arg) {
    let mut segments = url.path_segments().ok_or_else(bad)?;
    let id = match (segments.next(), segments.next()) {
      (Some("users"), Some(id)) if is_id(id) => id.to_owned(),
      _ => return Err(bad()),
    };
    return Ok(UserRef {
      site: site_of(&url),
      id: Some(id),
    });
  }
  let id = arg.strip_prefix("users/").unwrap_or(arg);
  let id = id.split('/').next().unwrap_or(id);
  if is_id(id) {
    return Ok(UserRef {
      site: None,
      id: Some(id.to_owned()),
    });
  }
  Err(bad())
}

/// A comment: its site (from a link) and id.
pub fn comment(arg: &str) -> Result<(Option<String>, String)> {
  let arg = arg.trim();
  if is_id(arg) {
    return Ok((None, arg.to_owned()));
  }
  let url = Url::parse(arg).ok();
  let id = url
    .as_ref()
    .and_then(|url| url.fragment()?.strip_prefix("comment"))
    .and_then(|rest| rest.split('_').next())
    .filter(|id| is_id(id));
  match (id, &url) {
    (Some(id), Some(url)) => Ok((site_of(url), id.to_owned())),
    _ => Err(Error::input(format!(
      "not a comment: {arg} (expected an id or a `#comment<id>_…` link)"
    ))),
  }
}
