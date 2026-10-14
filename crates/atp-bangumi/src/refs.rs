//! Subjects, characters, persons, episodes and users as arguments.
//!
//! - subjects: `https://bgm.tv/subject/<id>` (also bangumi.tv, chii.in and
//!   next.bgm.tv), `subject/<id>`, or the id;
//! - characters, persons, episodes: `/character/<id>`, `/person/<id>`,
//!   `/ep/<id>` links or paths;
//! - users: `https://bgm.tv/user/<name>`, `user/<name>`, `@name`, or the
//!   name (users without one go by their number).

use atp_core::{Error, Result};
use url::Url;

/// What a page on Bangumi shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
  Subject(u64),
  Character(u64),
  Person(u64),
  Episode(u64),
}

/// The path of a link (`subject/1`), or the argument itself.
fn path_of(arg: &str) -> String {
  let arg = arg.trim();
  match Url::parse(arg) {
    Ok(url) => url.path().trim_matches('/').to_owned(),
    Err(_) => arg.trim_matches('/').to_owned(),
  }
}

/// A subject, character, person or episode; a bare id is a subject.
pub fn item(arg: &str) -> Result<Item> {
  let path = path_of(arg);
  let (kind, id) = path.split_once('/').unwrap_or(("subject", &path));
  let id: u64 = id
    .split('/')
    .next()
    .and_then(|id| id.parse().ok())
    .ok_or_else(|| bad(arg))?;
  Ok(match kind {
    "subject" => Item::Subject(id),
    "character" => Item::Character(id),
    "person" => Item::Person(id),
    "ep" | "episode" => Item::Episode(id),
    _ => return Err(bad(arg)),
  })
}

fn bad(arg: &str) -> Error {
  Error::input(format!(
    "not a Bangumi subject, character, person or episode: {arg} (expected a link or an id)"
  ))
}

/// A subject's id.
pub fn subject(arg: &str) -> Result<u64> {
  match item(arg)? {
    Item::Subject(id) => Ok(id),
    _ => Err(Error::input(format!("not a Bangumi subject: {arg}"))),
  }
}

pub fn user(arg: &str) -> Result<String> {
  let path = path_of(arg);
  let name = path
    .strip_prefix("user/")
    .unwrap_or(&path)
    .split('/')
    .next()
    .unwrap_or_default()
    .trim_start_matches('@');
  let ok = !name.is_empty()
    && name
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
  if ok {
    Ok(name.to_owned())
  } else {
    Err(Error::input(format!(
      "not a Bangumi user: {arg} (expected a /user/ link or a user name)"
    )))
  }
}
