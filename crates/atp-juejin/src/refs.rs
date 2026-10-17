//! Articles, pins and users as arguments.
//!
//! - articles: `https://juejin.cn/post/<id>` (also juejin.im), `article:<id>`
//!   or a bare id;
//! - pins (沸点): `https://juejin.cn/pin/<id>` or `pin:<id>`;
//! - users: `https://juejin.cn/user/<id>` (and its sub-pages) or the id;
//! - collection sets (收藏集): `https://juejin.cn/collection/<id>` or the id.

use atp_core::{Error, Result};
use url::Url;

/// What a post argument names; the number is Juejin's `item_type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
  Article(String),
  Pin(String),
}

impl Item {
  pub fn id(&self) -> &str {
    match self {
      Item::Article(id) | Item::Pin(id) => id,
    }
  }

  /// Juejin's `item_type`: 2 for articles, 4 for pins.
  pub fn item_type(&self) -> u64 {
    match self {
      Item::Article(_) => 2,
      Item::Pin(_) => 4,
    }
  }
}

fn is_id(s: &str) -> bool {
  !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `(word, id)` of a juejin.cn link such as `/post/<id>`.
fn link(arg: &str) -> Option<(String, String)> {
  let url = Url::parse(arg).ok()?;
  let host = url.host_str()?;
  if !(host.ends_with("juejin.cn") || host.ends_with("juejin.im")) {
    return None;
  }
  let mut parts = url.path_segments()?.filter(|s| !s.is_empty());
  let word = parts.next()?.to_owned();
  let id = parts.next()?.to_owned();
  Some((word, id))
}

pub fn item(arg: &str) -> Result<Item> {
  let arg = arg.trim();
  let (word, id) = match link(arg) {
    Some(found) => found,
    None => match arg.split_once(':') {
      Some((word, id)) => (word.to_owned(), id.to_owned()),
      None => ("post".to_owned(), arg.to_owned()),
    },
  };
  match word.as_str() {
    "post" | "article" | "entry" if is_id(&id) => Ok(Item::Article(id)),
    "pin" | "pins" if is_id(&id) => Ok(Item::Pin(id)),
    _ => Err(Error::input(format!(
      "not a Juejin article or pin: {arg} (give a juejin.cn/post/ or /pin/ link, or an id)"
    ))),
  }
}

/// A collection set (收藏集): `https://juejin.cn/collection/<id>` or the id.
pub fn collection(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let id = match link(arg) {
    Some((word, id)) if word == "collection" => id,
    Some(_) => String::new(),
    None => arg.to_owned(),
  };
  if is_id(&id) {
    Ok(id)
  } else {
    Err(Error::input(format!(
      "not a Juejin collection set: {arg} (give a juejin.cn/collection/ link or the id)"
    )))
  }
}

pub fn user(arg: &str) -> Result<String> {
  let arg = arg.trim().trim_start_matches('@');
  let id = match link(arg) {
    Some((word, id)) if word == "user" => id,
    Some(_) => String::new(),
    None => arg.to_owned(),
  };
  if is_id(&id) {
    Ok(id)
  } else {
    Err(Error::input(format!(
      "not a Juejin user: {arg} (give a juejin.cn/user/ link or the user id)"
    )))
  }
}
