//! Books and users as arguments.
//!
//! - books: a book id (`695233`, `MP_WXS_…`, `CB_…`), a reader or detail
//!   link (`https://weread.qq.com/web/reader/<hash>`, `/web/bookDetail/<hash>`,
//!   `/book-detail?v=<hash>`), or `#N`;
//! - users: a user number (`vid`) or `@vid`.

use atp_core::{Error, Result};
use url::Url;

use crate::ids;

fn is_book_id(arg: &str) -> bool {
  !arg.is_empty()
    && arg
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The hash a WeRead link carries, if `arg` is one.
fn link_hash(arg: &str) -> Option<String> {
  let url = Url::parse(arg).ok()?;
  if !url.host_str()?.ends_with("weread.qq.com") {
    return None;
  }
  if let Some((_, v)) = url.query_pairs().find(|(k, _)| k == "v") {
    return Some(v.into_owned());
  }
  let last = url.path_segments()?.rev().find(|s| !s.is_empty())?;
  // A chapter link is `<book hash>k<chapter hash>`.
  Some(last.split('k').next().unwrap_or(last).to_owned())
}

pub fn book(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if let Some(hash) = link_hash(arg) {
    return ids::decode(&hash).ok_or_else(|| bad(arg));
  }
  if is_book_id(arg) {
    return Ok(arg.to_owned());
  }
  Err(bad(arg))
}

fn bad(arg: &str) -> Error {
  Error::input(format!(
    "not a WeRead book: {arg} (give a book id or a weread.qq.com reader or detail link)"
  ))
}

pub fn user(arg: &str) -> Result<String> {
  let vid = arg.trim().trim_start_matches('@');
  if !vid.is_empty() && vid.bytes().all(|b| b.is_ascii_digit()) {
    Ok(vid.to_owned())
  } else {
    Err(Error::input(format!(
      "not a WeRead user: {arg} (give the user number, `vid`)"
    )))
  }
}

/// The detail page of a book.
pub fn book_url(id: &str) -> String {
  format!("https://weread.qq.com/web/bookDetail/{}", ids::encode(id))
}
