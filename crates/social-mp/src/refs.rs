//! Article and album links.
//!
//! Articles come as short links (`/s/<id>`) or long ones
//! (`/s?__biz=..&mid=..&idx=..&sn=..&chksm=..`); anonymous reads of a long
//! link need its `chksm`, and the two forms do not convert into each other.
//! An article's own id is `<bizuin>_<mid>_<idx>` (the key album listings use).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use social_core::{Error, Result};
use url::Url;

pub const HOME: &str = "https://mp.weixin.qq.com";

/// A link to fetch an article page with, canonicalized: https, no fragment,
/// short links without their query, long ones without tracking parameters.
pub fn article_url(arg: &str) -> Result<String> {
  let arg = arg.trim().replace("&amp;", "&");
  let mut url = Url::parse(&arg)
    .ok()
    .filter(|u| u.host_str() == Some("mp.weixin.qq.com"))
    .ok_or_else(|| {
      Error::input(format!(
        "not a WeChat article link: {arg} (expected https://mp.weixin.qq.com/s/...)"
      ))
    })?;
  url.set_fragment(None);
  let _ = url.set_scheme("https");
  if let Some(id) = url.path().strip_prefix("/s/") {
    let id = id.trim_end_matches('/').to_owned();
    return Ok(format!("{HOME}/s/{id}"));
  }
  if url.path() != "/s" {
    return Err(Error::input(format!("not an article link: {arg}")));
  }
  let keep = ["__biz", "mid", "idx", "sn", "chksm"];
  let pairs: Vec<(String, String)> = url
    .query_pairs()
    .filter(|(k, v)| keep.contains(&k.as_ref()) && !v.is_empty())
    .map(|(k, v)| (k.into_owned(), v.into_owned()))
    .collect();
  if !pairs.iter().any(|(k, _)| k == "__biz") {
    return Err(Error::input(format!("article link without __biz: {arg}")));
  }
  url.query_pairs_mut().clear().extend_pairs(pairs);
  Ok(url.into())
}

/// The decimal account id behind a `__biz` value (base64 of its digits).
pub fn bizuin(biz: &str) -> Option<String> {
  let raw = STANDARD.decode(biz.trim()).ok()?;
  let s = String::from_utf8(raw).ok()?;
  s.bytes().all(|b| b.is_ascii_digit()).then_some(s)
}

/// `(biz, album_id)` of an album link or `biz/album_id`.
pub fn album(arg: &str) -> Result<(String, String)> {
  let arg = arg.trim().replace("&amp;", "&");
  if let Ok(url) = Url::parse(&arg) {
    let get = |name: &str| {
      url
        .query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
    };
    if let (Some(biz), Some(id)) = (get("__biz"), get("album_id")) {
      return Ok((biz, id));
    }
  } else if let Some((biz, id)) = arg.split_once('/')
    && id.bytes().all(|b| b.is_ascii_digit())
  {
    return Ok((biz.to_owned(), id.to_owned()));
  }
  Err(Error::input(format!(
    "not an album: {arg} (expected an mp/appmsgalbum link with __biz and album_id)"
  )))
}

pub fn album_url(biz: &str, album_id: &str) -> String {
  let mut url = Url::parse(&format!("{HOME}/mp/appmsgalbum")).expect("valid");
  url
    .query_pairs_mut()
    .append_pair("__biz", biz)
    .append_pair("action", "getalbum")
    .append_pair("album_id", album_id);
  url.into()
}
