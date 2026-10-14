//! Links and ids.
//!
//! Items are SKUs (numbers of five digits or more) behind
//! `item.jd.com/<sku>.html`, `item.m.jd.com/product/<sku>.html`,
//! `item.jd.hk/<sku>.html`, `npcitem.jd.hk/<sku>.html`, links with a
//! `sku=` / `skuId=` / `wareId=` parameter, and the `3.cn` / `u.jd.com`
//! short links, which are followed. Shops are their ids behind
//! `mall.jd.com/index-<id>.html` or `shop.jd.com/index-<id>.html`.

use std::sync::LazyLock;

use atp_core::{Error, Result};
use regex::Regex;

use crate::api::Api;

static ITEM_PATH: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?:item(?:\.m)?\.jd\.(?:com|hk)|npcitem\.jd\.hk)/(?:product/)?(\d{4,})\.html")
    .expect("valid")
});
static ITEM_PARAM: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"[?&](?:sku|skuId|wareId)=(\d{4,})").expect("valid"));
static SHOP_PATH: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?:mall|shop)\.jd\.(?:com|hk)/index-(\d+)\.html|[?&]shopId=(\d+)").expect("valid")
});
static DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4,}$").expect("valid"));

/// The SKU of an item reference (`#N` already resolved).
pub async fn sku(api: &Api, arg: &str) -> Result<String> {
  let arg = arg.trim();
  if DIGITS.is_match(arg) {
    return Ok(arg.to_owned());
  }
  if let Some(sku) = sku_in(arg) {
    return Ok(sku);
  }
  if arg.contains("3.cn/") || arg.contains("u.jd.com/") {
    let link = api.ctx.http.final_url(arg).await?;
    if let Some(sku) = sku_in(&link) {
      return Ok(sku);
    }
  }
  Err(Error::input(format!(
    "not a JD item: {arg} (give a SKU or an item.jd.com link)"
  )))
}

fn sku_in(text: &str) -> Option<String> {
  ITEM_PATH
    .captures(text)
    .or_else(|| ITEM_PARAM.captures(text))
    .map(|m| m[1].to_owned())
}

/// The id of a shop reference.
pub fn shop(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if arg.chars().all(|c| c.is_ascii_digit()) && !arg.is_empty() {
    return Ok(arg.to_owned());
  }
  SHOP_PATH
    .captures(arg)
    .and_then(|m| m.get(1).or(m.get(2)))
    .map(|m| m.as_str().to_owned())
    .ok_or_else(|| {
      Error::input(format!(
        "not a JD shop: {arg} (give a shop id or a mall.jd.com/index-<id>.html link)"
      ))
    })
}
