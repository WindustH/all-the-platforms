//! What arguments name on Douban.
//!
//! - subjects: `movie.douban.com/subject/<id>/` (films and TV), `book.…`,
//!   `music.…`, `m.douban.com/<type>/subject/<id>/`, `douban://douban.com/<type>/<id>`
//!   app links, `movie:<id>` (also `tv:`, `book:`, `music:`) or a bare id
//!   (a film or TV show);
//! - reviews: `…/review/<id>/` links or `review:<id>`;
//! - group topics: `…/group/topic/<id>/` links or `topic:<id>`; the
//!   `_spm_id` that Douban's own lists put in a topic's link is kept, since
//!   topics are only served with it;
//! - groups: `…/group/<id or name>/` links or `group:<id>`;
//! - users: `…/people/<uid or id>/` links, `@uid`, the uid or the number.

use atp_core::{Error, Result};
use url::Url;

/// The subject types the API serves under their own path.
pub const TYPES: &[&str] = &["movie", "tv", "book", "music"];

/// A film, TV show, book or album.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
  /// `movie`, `tv`, `book` or `music`.
  pub kind: String,
  pub id: String,
}

/// A group topic, with the `_spm_id` of the link it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
  pub id: String,
  pub spm: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
  Subject(Subject),
  Review(String),
  Topic(Topic),
  Group(String),
}

fn topic(id: &str, spm: Option<String>) -> Option<Target> {
  is_id(id).then(|| {
    Target::Topic(Topic {
      id: id.to_owned(),
      spm,
    })
  })
}

/// The `_spm_id` of a link, if it has one.
fn spm_of(arg: &str) -> Option<String> {
  Url::parse(arg.trim())
    .ok()?
    .query_pairs()
    .find(|(k, _)| k == "_spm_id")
    .map(|(_, v)| v.into_owned())
    .filter(|v| !v.is_empty())
}

/// The path segments of a link (host first), or `None` for plain words.
fn segments(arg: &str) -> Option<Vec<String>> {
  let url = Url::parse(arg.trim()).ok()?;
  let host = url.host_str().unwrap_or_default().to_owned();
  if !host.ends_with("douban.com") {
    return None;
  }
  let mut parts = vec![host];
  parts.extend(
    url
      .path()
      .split('/')
      .filter(|s| !s.is_empty())
      .map(str::to_owned),
  );
  Some(parts)
}

fn is_id(s: &str) -> bool {
  !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn subject(kind: &str, id: &str) -> Option<Target> {
  is_id(id).then(|| {
    Target::Subject(Subject {
      kind: kind.to_owned(),
      id: id.to_owned(),
    })
  })
}

/// The kind a `<site>.douban.com` host serves (`movie` covers TV too).
fn site_kind(host: &str) -> &'static str {
  match host.split('.').next() {
    Some("book") => "book",
    Some("music") => "music",
    _ => "movie",
  }
}

/// What a douban.com link points to.
fn from_link(parts: &[String], spm: Option<String>) -> Option<Target> {
  let host = parts.first()?.as_str();
  let path: Vec<&str> = parts[1..].iter().map(String::as_str).collect();
  let id_after = |word: &str| {
    path
      .iter()
      .position(|p| *p == word)
      .and_then(|i| path.get(i + 1))
      .map(|s| (*s).to_owned())
  };
  if let Some(id) = id_after("topic").filter(|_| path.contains(&"group")) {
    return topic(&id, spm);
  }
  if let Some(id) = id_after("review") {
    return is_id(&id).then_some(Target::Review(id));
  }
  if let Some(group) = id_after("group") {
    return Some(Target::Group(group));
  }
  if let Some(id) = id_after("subject") {
    let kind = match path.first() {
      Some(kind) if TYPES.contains(kind) => kind,
      _ => site_kind(host),
    };
    return subject(kind, &id);
  }
  // App links and dispatch pages: `…/<type>/<id>`.
  path
    .windows(2)
    .find(|w| TYPES.contains(&w[0]) && is_id(w[1]))
    .and_then(|w| subject(w[0], w[1]))
}

/// `douban://douban.com/<type>/<id>` as an https link, for [`from_link`].
fn app_link(arg: &str) -> String {
  arg
    .trim()
    .replacen("douban://douban.com/", "https://www.douban.com/", 1)
}

pub fn target(arg: &str) -> Result<Target> {
  let arg = app_link(arg);
  if let Some(parts) = segments(&arg) {
    return from_link(&parts, spm_of(&arg)).ok_or_else(|| bad(&arg));
  }
  let (kind, id) = arg.split_once(':').unwrap_or(("movie", &arg));
  let id = id.trim();
  let found = match kind {
    "review" => is_id(id).then(|| Target::Review(id.to_owned())),
    "topic" => topic(id, None),
    "group" => (!id.is_empty()).then(|| Target::Group(id.to_owned())),
    kind if TYPES.contains(&kind) => subject(kind, id),
    _ => None,
  };
  found.ok_or_else(|| bad(&arg))
}

fn bad(arg: &str) -> Error {
  Error::input(format!(
    "not a Douban subject, review, group or topic: {arg} (give a link, `movie:ID`, \
     `book:ID`, `music:ID`, `review:ID`, `topic:ID`, `group:ID` or a film id)"
  ))
}

pub fn subject_ref(arg: &str) -> Result<Subject> {
  match target(arg)? {
    Target::Subject(subject) => Ok(subject),
    _ => Err(Error::input(format!(
      "not a Douban film, show, book or album: {arg}"
    ))),
  }
}

pub fn group(arg: &str) -> Result<String> {
  match target(&normalize_group(arg))? {
    Target::Group(id) => Ok(id),
    _ => Err(Error::input(format!("not a Douban group: {arg}"))),
  }
}

/// A bare group id or name is a group here, not a film.
fn normalize_group(arg: &str) -> String {
  if arg.contains(':') || arg.contains('/') {
    arg.to_owned()
  } else {
    format!("group:{arg}")
  }
}

/// A user's uid or number.
pub fn user(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let name = match segments(arg) {
    Some(parts) => parts
      .iter()
      .position(|p| p == "people" || p == "user")
      .and_then(|i| parts.get(i + 1))
      .cloned()
      .unwrap_or_default(),
    None => arg.trim_start_matches('@').to_owned(),
  };
  let ok = !name.is_empty()
    && name
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.');
  if ok {
    Ok(name)
  } else {
    Err(Error::input(format!(
      "not a Douban user: {arg} (give a /people/ link, a uid or a user number)"
    )))
  }
}
