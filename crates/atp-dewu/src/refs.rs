//! Links and ids.
//!
//! Products are `spuId`s: bare (`1234567`, or `spu:1234567`) or in a
//! link with `spuId=` on www.dewu.com (`product-detail.html`),
//! `m.dewu.com` / `cdn-m.dewu.com` (`router/product/ProductDetail`) or
//! `fast.dewu.com` / `cdn-fast.dewu.com` (`page/productDetail`).
//! Community posts are `trendId`s: `trend:514124663`, or a share link
//! (`m.dewu.com/rn-activity/community-share?trendId=`, also
//! `/h5-sociality/community/community-share`). Short links (`dw4.co/t/…`)
//! and other `fast.dewu.com` / `cdn-fast.dewu.com` links without an id are
//! followed to where they lead.

use atp_core::{Error, Result};
use url::Url;

use crate::api::Api;

/// What a reference points at.
pub enum Ref {
  /// A product (`spuId`).
  Product(String),
  /// A community post (`trendId`).
  Trend(String),
}

/// Hosts whose links atp reads.
const HOSTS: &[&str] = &["dewu.com", "poizon.com", "dw4.co"];
/// Hosts of links that may only redirect to the real page.
const SHORT: &[&str] = &["dw4.co", "fast.dewu.com", "cdn-fast.dewu.com"];

/// The product or post of an argument (`#N` already resolved).
pub async fn parse(api: &Api, arg: &str) -> Result<Ref> {
  let arg = arg.trim();
  if let Some(found) = plain(arg) {
    return Ok(found);
  }
  let url = Url::parse(arg).map_err(|_| unknown(arg))?;
  let host = url.host_str().unwrap_or_default().to_owned();
  if !on(&host, HOSTS) {
    return Err(unknown(arg));
  }
  if let Some(found) = in_link(&url) {
    return Ok(found);
  }
  if on(&host, SHORT) {
    let link = api.ctx.http.final_url(arg).await?;
    if let Some(found) = Url::parse(&link).ok().as_ref().and_then(in_link) {
      return Ok(found);
    }
  }
  Err(unknown(arg))
}

/// `123`, `spu:123`, `trend:123`.
fn plain(arg: &str) -> Option<Ref> {
  let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
  if let Some(id) = arg.strip_prefix("trend:").filter(|s| digits(s)) {
    return Some(Ref::Trend(id.to_owned()));
  }
  let id = arg.strip_prefix("spu:").unwrap_or(arg);
  digits(id).then(|| Ref::Product(id.to_owned()))
}

/// The id a link carries: `trendId=` or `spuId=`.
fn in_link(url: &Url) -> Option<Ref> {
  let param = |name: &str| {
    url
      .query_pairs()
      .find(|(k, v)| k.eq_ignore_ascii_case(name) && !v.is_empty())
      .map(|(_, v)| v.into_owned())
      .filter(|v| v.bytes().all(|b| b.is_ascii_digit()))
  };
  if let Some(trend) = param("trendId") {
    return Some(Ref::Trend(trend));
  }
  param("spuId").map(Ref::Product)
}

fn on(host: &str, domains: &[&str]) -> bool {
  domains
    .iter()
    .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

fn unknown(arg: &str) -> Error {
  Error::input(format!(
    "not a Dewu product or post: {arg} (give a spuId, trend:ID, or a dewu.com / dw4.co link)"
  ))
}
