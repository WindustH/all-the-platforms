//! What file arguments name: a file id (`fid`, 32 hex digits), a path in
//! the drive (`/Movies/a.mkv`; `/` or `0` for the root), or a share link
//! (`https://pan.quark.cn/s/<id>`, its code in `?pwd=` or as `提取码：abcd`
//! in pasted text).

use std::sync::LazyLock;

use atp_core::{Error, Result};
use regex::Regex;

use crate::api::PAN;

static SHARE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"pan\.quark\.cn/s/([0-9A-Za-z]+)").expect("valid"));
static CODE: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?i)(?:[?&]pwd=|提取码\s*[:：]?\s*|密码\s*[:：]?\s*)([0-9A-Za-z]{4,8})")
    .expect("valid")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRef {
  Id(String),
  /// Absolute, without a trailing `/`; `/` is the root.
  Path(String),
  Share(ShareRef),
}

/// Someone's share: its id (`pwd_id`) and the code it asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareRef {
  pub id: String,
  pub code: Option<String>,
}

impl ShareRef {
  pub fn url(&self) -> String {
    format!("{PAN}/s/{}", self.id)
  }
}

pub fn file(arg: &str) -> Result<FileRef> {
  let arg = arg.trim();
  if arg.is_empty() {
    return Err(Error::input("expected a path, a file id or a share link"));
  }
  if SHARE.is_match(arg) {
    return share(arg, None).map(FileRef::Share);
  }
  if arg.contains("://") {
    return Err(Error::input(format!("not a Quark Drive link: {arg}")));
  }
  if arg == "0" {
    return Ok(FileRef::Path("/".into()));
  }
  if arg.len() == 32 && arg.bytes().all(|b| b.is_ascii_hexdigit()) {
    return Ok(FileRef::Id(arg.to_ascii_lowercase()));
  }
  Ok(FileRef::Path(path(arg)))
}

/// A share link; `code` (from `--pwd`) wins over one in the text.
pub fn share(arg: &str, code: Option<String>) -> Result<ShareRef> {
  let id = SHARE
    .captures(arg)
    .map(|c| c[1].to_owned())
    .ok_or_else(|| Error::input(format!("not a Quark share link: {arg}")))?;
  let found = CODE.captures(arg).map(|c| c[1].to_owned());
  Ok(ShareRef {
    id,
    code: code.filter(|c| !c.trim().is_empty()).or(found),
  })
}

/// `a/b/`, `/a//b` → `/a/b`.
pub fn path(arg: &str) -> String {
  let parts: Vec<&str> = arg.split('/').filter(|p| !p.is_empty()).collect();
  format!("/{}", parts.join("/"))
}

/// The folder part and the name of a path: `/a/b` → (`/a`, `b`).
pub fn split(path: &str) -> (String, String) {
  match path.rsplit_once('/') {
    Some(("", name)) => ("/".into(), name.into()),
    Some((parent, name)) => (parent.into(), name.into()),
    None => ("/".into(), path.into()),
  }
}

/// `parent` + `/` + `name`.
pub fn join(parent: &str, name: &str) -> String {
  format!("{}/{name}", parent.trim_end_matches('/'))
}
