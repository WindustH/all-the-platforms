//! Links and ids.
//!
//! Goods are goods ids (numbers) behind `goods.html?goods_id=N` (also
//! `goods1.html`, `goods2.html`, on `mobile.yangkeduo.com` or
//! `mobile.pinduoduo.com`), or 多多进宝 `goods_sign`s. Share links
//! (`goods.html?ps=…`, `p.pinduoduo.com/…`) are followed. Shops are mall
//! ids behind `mall_page.html?mall_id=N`.

use std::sync::LazyLock;

use atp_core::{Error, Result};
use regex::Regex;

use crate::api::Api;

static GOODS_ID: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"[?&#](?:goods_id|goodsId)=(\d{5,})").expect("valid"));
static GOODS_SIGN: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"[?&#]goods_sign=([A-Za-z0-9_-]{16,})").expect("valid"));
static SIGN: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]{20,64}$").expect("valid"));
static MALL_ID: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"[?&#](?:mall_id|mallId)=(\d+)").expect("valid"));

/// A goods reference.
pub enum Goods {
  /// A goods id of the H5 site.
  Id(String),
  /// A 多多进宝 `goods_sign`.
  Sign(String),
}

/// The goods of an argument (`#N` already resolved).
pub async fn goods(api: &Api, arg: &str) -> Result<Goods> {
  let arg = arg.trim();
  if arg.len() >= 5 && arg.chars().all(|c| c.is_ascii_digit()) {
    return Ok(Goods::Id(arg.to_owned()));
  }
  if let Some(found) = goods_in(arg) {
    return Ok(found);
  }
  if SIGN.is_match(arg) && arg.chars().any(|c| c.is_ascii_alphabetic()) {
    return Ok(Goods::Sign(arg.to_owned()));
  }
  if arg.starts_with("http") && (arg.contains("yangkeduo.com") || arg.contains("pinduoduo.com")) {
    let link = api.ctx.http.final_url(arg).await?;
    if let Some(found) = goods_in(&link) {
      return Ok(found);
    }
  }
  Err(Error::input(format!(
    "not a Pinduoduo goods: {arg} (give a goods id, a goods.html link or a goods_sign)"
  )))
}

fn goods_in(text: &str) -> Option<Goods> {
  if let Some(m) = GOODS_ID.captures(text) {
    return Some(Goods::Id(m[1].to_owned()));
  }
  GOODS_SIGN
    .captures(text)
    .map(|m| Goods::Sign(m[1].to_owned()))
}

/// The mall id of a shop reference.
pub fn mall(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if !arg.is_empty() && arg.chars().all(|c| c.is_ascii_digit()) {
    return Ok(arg.to_owned());
  }
  MALL_ID
    .captures(arg)
    .map(|m| m[1].to_owned())
    .ok_or_else(|| {
      Error::input(format!(
        "not a Pinduoduo shop: {arg} (give a mall id or a mall_page.html?mall_id= link)"
      ))
    })
}
