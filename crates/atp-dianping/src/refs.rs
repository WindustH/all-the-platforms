//! References. Shops: a numeric id (`563754`), a uuid
//! (`H4KBo5JMg48nZcSa`) or a `www`/`m.dianping.com/shop/…` link (see
//! [`atp_mtsec::shops`]). Notes: `m.dianping.com/discovery/<id>` or
//! `www.dianping.com/note/<id>`. Members: a numeric id or
//! `www.dianping.com/member/<id>`.

use std::sync::LazyLock;

use atp_core::{Error, Result};
use atp_mtsec::shops::{self, Shop};
use regex::Regex;

use crate::api::{MOBILE, WWW};

static NOTE: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"dianping\.com/(?:discovery|note|ugcdetail)/(\d{4,})").expect("valid")
});
static MEMBER: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"dianping\.com/member/(\d+)").expect("valid"));

pub enum Target {
  Shop(Shop),
  Note(String),
}

pub fn target(arg: &str) -> Result<Target> {
  if let Some(m) = NOTE.captures(arg) {
    return Ok(Target::Note(m[1].to_owned()));
  }
  shops::parse(arg).map(Target::Shop).ok_or_else(|| {
    Error::input(format!(
      "not a Dianping shop or note: {arg} (give a shop id, a shop link or a discovery link)"
    ))
  })
}

pub fn shop(arg: &str) -> Result<Shop> {
  shops::parse(arg).ok_or_else(|| {
    Error::input(format!(
      "not a Dianping shop: {arg} (give a shop id, its uuid or a dianping.com/shop/… link)"
    ))
  })
}

pub fn member(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if !arg.is_empty() && arg.chars().all(|c| c.is_ascii_digit()) {
    return Ok(arg.to_owned());
  }
  MEMBER
    .captures(arg)
    .map(|m| m[1].to_owned())
    .ok_or_else(|| Error::input(format!("not a Dianping member: {arg}")))
}

pub fn note_url(id: &str) -> String {
  format!("{MOBILE}/discovery/{id}")
}

pub fn member_url(id: &str) -> String {
  format!("{WWW}/member/{id}")
}
