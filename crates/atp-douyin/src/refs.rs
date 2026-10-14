//! References: aweme ids, `douyin.com/video|note/<id>` and `?modal_id=`
//! links, `iesdouyin.com/share/...` pages, `v.douyin.com` short links (or
//! shared text containing one) and users as `sec_uid`, `/user/<sec_uid>`
//! links or `@unique_id`.

use std::sync::LazyLock;

use atp_core::{Error, Result};
use regex::Regex;

use crate::api::Api;
use crate::api::WWW;

static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https?://[^\s，。！？、]+").unwrap());
static AWEME_IN_URL: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?:/video/|/note/|/slides/|/share/video/|/share/note/|/share/slides/|[?&]modal_id=|[?&]vid=|[?&]aweme_id=)(\d{15,21})").unwrap()
});
static USER_IN_URL: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?:/user/|/share/user/|[?&]sec_uid=|[?&]sec_user_id=)([A-Za-z0-9_\-]{20,})").unwrap()
});
static DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{15,21}$").unwrap());

pub fn video_url(id: &str) -> String {
  format!("{WWW}/video/{id}")
}

pub fn note_url(id: &str) -> String {
  format!("{WWW}/note/{id}")
}

pub fn user_url(sec_uid: &str) -> String {
  format!("{WWW}/user/{sec_uid}")
}

/// The first link of a shared text, short links followed to their target.
async fn link(api: &Api, arg: &str) -> Result<Option<String>> {
  let Some(m) = URL.find(arg) else {
    return Ok(None);
  };
  let url = m.as_str().to_owned();
  if url.contains("v.douyin.com") || url.contains("iesdouyin.com/share") && url.contains("/s/") {
    return Ok(Some(api.ctx.http.final_url(&url).await?));
  }
  Ok(Some(url))
}

/// The aweme id of a post reference.
pub async fn aweme(api: &Api, arg: &str) -> Result<String> {
  let arg = arg.trim();
  if DIGITS.is_match(arg) {
    return Ok(arg.to_owned());
  }
  if let Some(url) = link(api, arg).await?
    && let Some(c) = AWEME_IN_URL.captures(&url)
  {
    return Ok(c[1].to_owned());
  }
  Err(Error::input(format!(
    "not a Douyin post: {arg} (expected an id, a douyin.com/video/… link or a v.douyin.com short link)"
  )))
}

/// A user reference as given: a `sec_uid`, or `@unique_id` / a search term
/// to resolve by search.
pub enum UserArg {
  SecUid(String),
  Handle(String),
}

pub async fn user(api: &Api, arg: &str) -> Result<UserArg> {
  let arg = arg.trim();
  if let Some(handle) = arg.strip_prefix('@') {
    return Ok(UserArg::Handle(handle.to_owned()));
  }
  if arg.starts_with("MS4wLjABAAAA") {
    return Ok(UserArg::SecUid(arg.to_owned()));
  }
  if let Some(url) = link(api, arg).await? {
    if let Some(c) = USER_IN_URL.captures(&url) {
      return Ok(UserArg::SecUid(c[1].to_owned()));
    }
    return Err(Error::input(format!("not a Douyin user link: {arg}")));
  }
  if !arg.is_empty() && !arg.contains(char::is_whitespace) {
    return Ok(UserArg::Handle(arg.to_owned()));
  }
  Err(Error::input(format!(
    "not a Douyin user: {arg} (expected a sec_uid, a douyin.com/user/… link or @douyin_id)"
  )))
}

/// A mix (合集) id from an id or `douyin.com/collection/<id>` link.
pub fn mix(arg: &str) -> Result<String> {
  id_after(arg, &["/collection/", "/mix/", "mix_id="])
    .ok_or_else(|| Error::input(format!("not a Douyin mix: {arg}")))
}

/// A music id from an id or `douyin.com/music/<id>` link.
pub fn music(arg: &str) -> Result<String> {
  id_after(arg, &["/music/", "music_id="])
    .ok_or_else(|| Error::input(format!("not a Douyin music: {arg}")))
}

/// A live room's `web_rid` from an id, `live:<id>` or a
/// `live.douyin.com/<id>` link.
pub fn live(arg: &str) -> Result<String> {
  id_after(
    arg,
    &["live.douyin.com/", "/follow/live/", "/live/", "live:"],
  )
  .ok_or_else(|| Error::input(format!("not a Douyin live room: {arg}")))
}

fn id_after(arg: &str, markers: &[&str]) -> Option<String> {
  let arg = arg.trim();
  if !arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit()) {
    return Some(arg.to_owned());
  }
  markers.iter().find_map(|m| {
    let rest = &arg[arg.find(m)? + m.len()..];
    let id: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (!id.is_empty()).then_some(id)
  })
}
