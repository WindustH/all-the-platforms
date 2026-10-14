//! Dianping shops: references, and the mobile shop page.
//!
//! A shop has a numeric id (`563754`), by which the mobile page
//! `m.dianping.com/shop/<id>` still opens to visitors, and a uuid
//! (`H4KBo5JMg48nZcSa`) that the www site and the review list use. The
//! mobile page names the uuid (`shopuuid=` in its `shopServeInfoUrl`);
//! atp remembers the pairs it has seen.

use std::sync::LazyLock;
use std::time::Duration;

use atp_core::{Ctx, Error, Result};
use regex::Regex;

use crate::guard::{self, Call, DIANPING};

pub const WWW: &str = "https://www.dianping.com";
pub const MOBILE: &str = "https://m.dianping.com";
const UUID_TTL: Duration = Duration::from_secs(90 * 24 * 3600);

static PATH: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"dianping\.com/shop/([A-Za-z0-9]+)").expect("valid"));
static PARAM: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"[?&](?:shopuuid|shopUuid|referid|shopId|shopid)=([A-Za-z0-9]+)").expect("valid")
});
static UUID_IN: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"shopuuid=([A-Za-z0-9]{8,40})").expect("valid"));

/// A shop as given: its numeric id or its uuid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shop {
  Id(String),
  Uuid(String),
}

/// A shop id, uuid or link (`www`/`m.dianping.com/shop/…`, links with
/// `shopuuid=` or `shopId=`).
pub fn parse(arg: &str) -> Option<Shop> {
  let arg = arg.trim();
  let key = PATH
    .captures(arg)
    .or_else(|| PARAM.captures(arg))
    .map(|m| m[1].to_owned())
    .unwrap_or_else(|| arg.to_owned());
  if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric()) {
    return None;
  }
  if key.chars().all(|c| c.is_ascii_digit()) {
    return Some(Shop::Id(key));
  }
  (key.len() >= 8).then_some(Shop::Uuid(key))
}

/// The shop's page on the www site.
pub fn www_url(uuid: &str) -> String {
  format!("{WWW}/shop/{uuid}")
}

/// The shop's page on the mobile site, by numeric id.
pub fn mobile_url(id: &str) -> String {
  format!("{MOBILE}/shop/{id}")
}

/// The mobile shop page, as a phone loads it.
pub async fn page(ctx: &Ctx, id: &str) -> Result<String> {
  let call = Call::page(mobile_url(id), format!("{MOBILE}/")).mobile();
  guard::html(ctx, &DIANPING, call).await
}

/// The uuid a shop page names.
pub fn uuid_in(html: &str) -> Option<String> {
  UUID_IN.captures(html).map(|m| m[1].to_owned())
}

/// The uuid of a shop: as given, remembered, or read from its mobile page.
pub async fn uuid(ctx: &Ctx, shop: &Shop) -> Result<String> {
  let id = match shop {
    Shop::Uuid(uuid) => return Ok(uuid.clone()),
    Shop::Id(id) => id,
  };
  if let Some(uuid) = ctx.store.cache_get::<String>(&cache_key(id), UUID_TTL) {
    return Ok(uuid);
  }
  let html = page(ctx, id).await?;
  let uuid = uuid_in(&html)
    .ok_or_else(|| Error::upstream(format!("Dianping's page of shop {id} names no uuid")))?;
  remember(ctx, id, &uuid);
  Ok(uuid)
}

/// Keep a shop's id and uuid together.
pub fn remember(ctx: &Ctx, id: &str, uuid: &str) {
  ctx.store.cache_put(&cache_key(id), &uuid);
}

fn cache_key(id: &str) -> String {
  format!("dianping-shop-uuid-{id}")
}
