//! References: photo ids (`3x…`), `kuaishou.com/short-video/<id>`,
//! `/fw/photo/<id>` share pages, `live.kuaishou.com/u/<user>/<id>` links,
//! `v.kuaishou.com` short links (or shared text containing one); users as
//! eids (`3x…`) or `/profile/<eid>` links; live rooms as the anchor's id or
//! a `live.kuaishou.com/u/<id>` link.

use std::sync::LazyLock;

use atp_core::{Error, Result};
use regex::Regex;

use crate::api::{Api, LIVE, WWW};
use crate::sign::component;

static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https?://[^\s，。！？、]+").unwrap());
static PHOTO_IN_URL: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(
    r"(?:/short-video/|/fw/photo/|/photo/|/video/|live\.kuaishou\.com/u/[^/?#]+/|[?&]photoId=|[?&]shareObjectId=)([0-9A-Za-z_-]{6,})",
  )
  .unwrap()
});
static USER_IN_URL: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(?:/profile/|/u/|[?&]userId=)([0-9A-Za-z_-]+)").unwrap());
static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9A-Za-z_-]{3,}$").unwrap());
/// Hosts of share links that redirect to the post or profile.
const SHORT_HOSTS: &[&str] = &[
  "://v.kuaishou.com/",
  "://c.kuaishou.com/",
  "://kuaishou.cn/",
  "://www.kuaishou.com/f/",
  "://v.m.chenzhongtech.com/s/",
];

pub fn photo_url(id: &str) -> String {
  format!("{WWW}/short-video/{id}")
}

pub fn profile_url(eid: &str) -> String {
  format!("{WWW}/profile/{eid}")
}

pub fn live_url(id: &str) -> String {
  format!("{LIVE}/u/{id}")
}

pub fn search_url(keyword: &str) -> String {
  format!("{WWW}/search/video?searchKey={}", component(keyword))
}

/// The first link of a shared text, share links followed to their target.
async fn link(api: &Api, arg: &str) -> Result<Option<String>> {
  let Some(m) = URL.find(arg) else {
    return Ok(None);
  };
  let url = m.as_str();
  if SHORT_HOSTS.iter().any(|h| url.contains(h)) {
    return Ok(Some(api.ctx.http.final_url(url).await?));
  }
  Ok(Some(url.to_owned()))
}

/// The photo id of a post reference.
pub async fn photo(api: &Api, arg: &str) -> Result<String> {
  let arg = arg.trim();
  if ID.is_match(arg) {
    return Ok(arg.to_owned());
  }
  if let Some(url) = link(api, arg).await?
    && let Some(c) = PHOTO_IN_URL.captures(&url)
  {
    return Ok(c[1].to_owned());
  }
  Err(Error::input(format!(
    "not a Kuaishou post: {arg} (expected an id, a kuaishou.com/short-video/… link or a v.kuaishou.com share link)"
  )))
}

/// The eid of a user reference (`me` is left to the caller).
pub async fn user(api: &Api, arg: &str) -> Result<String> {
  let arg = arg.trim();
  if ID.is_match(arg) {
    return Ok(arg.to_owned());
  }
  if let Some(url) = link(api, arg).await? {
    if url.contains("live.kuaishou.com") {
      return Err(Error::input(format!(
        "{arg} is a live room; Kuaishou keeps profile ids apart from live ids, pass a kuaishou.com/profile/… link"
      )));
    }
    if let Some(c) = USER_IN_URL.captures(&url) {
      return Ok(c[1].to_owned());
    }
  }
  Err(Error::input(format!(
    "not a Kuaishou user: {arg} (expected an id like 3x… or a kuaishou.com/profile/… link)"
  )))
}

/// A live room: the anchor's live id, from an id or a `live.kuaishou.com/u/<id>` link.
pub fn room(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let arg = arg.strip_prefix("live:").unwrap_or(arg);
  if ID.is_match(arg) {
    return Ok(arg.to_owned());
  }
  USER_IN_URL
    .captures(arg)
    .map(|c| c[1].to_owned())
    .ok_or_else(|| {
      Error::input(format!(
        "not a Kuaishou live room: {arg} (expected the anchor's id or a live.kuaishou.com/u/… link)"
      ))
    })
}
