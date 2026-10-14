//! What file arguments name: a path in the drive (`/Movies/a.mkv`; `/` is
//! the root, relative paths start there), a file id (`fs_id`, a bare
//! number of 9 or more digits), or a share link
//! (`https://pan.baidu.com/s/1<key>` or `…/share/init?surl=<key>`, its code
//! in `?pwd=` or after `提取码：` in pasted text).

use std::sync::LazyLock;

use atp_core::{Error, Result};
use regex::Regex;

use crate::api::PAN;

static SHARE: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?:pan|yun)\.baidu\.com/(?:s/1|share/init\?(?:.*&)?surl=)([0-9A-Za-z_-]+)")
    .expect("valid")
});
static CODE: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?i)(?:[?&]pwd=|提取码\s*[:：]?\s*|密码\s*[:：]?\s*)([0-9A-Za-z]{4})")
    .expect("valid")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRef {
  /// Absolute, without a trailing `/`; `/` is the root.
  Path(String),
  /// A file id (`fs_id`).
  Id(String),
  Share(ShareRef),
}

/// Someone's share: its short key (`surl`, without the leading `1` of
/// `/s/1…`) and the code it asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareRef {
  pub surl: String,
  pub code: Option<String>,
}

impl ShareRef {
  pub fn url(&self) -> String {
    format!("{PAN}/s/1{}", self.surl)
  }

  /// The page that asks for the code, the `Referer` of the code check.
  pub fn init_url(&self) -> String {
    format!("{PAN}/share/init?surl={}", self.surl)
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
    return Err(Error::input(format!("not a Baidu Netdisk link: {arg}")));
  }
  if arg.len() >= 9 && arg.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(FileRef::Id(arg.to_owned()));
  }
  Ok(FileRef::Path(path(arg)))
}

/// A share link; `code` (from `--pwd`) wins over one in the text.
pub fn share(arg: &str, code: Option<String>) -> Result<ShareRef> {
  let surl = SHARE
    .captures(arg)
    .map(|c| c[1].to_owned())
    .ok_or_else(|| Error::input(format!("not a Baidu Netdisk share link: {arg}")))?;
  let found = CODE.captures(arg).map(|c| c[1].to_owned());
  Ok(ShareRef {
    surl,
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

/// A name for a file or folder: not empty, no `/` or other characters
/// Baidu refuses.
pub fn name(name: &str) -> Result<String> {
  let name = name.trim();
  let bad = name.is_empty()
    || name == "."
    || name == ".."
    || name
      .chars()
      .any(|c| r#"/\:*?"<>|"#.contains(c) || c.is_control());
  if bad {
    return Err(Error::input(format!(
      "`{name}` is not a valid name (no / \\ : * ? \" < > |)"
    )));
  }
  Ok(name.to_owned())
}
