//! Posts, users, hashtags and locations as arguments.
//!
//! Posts: a link (`/p/CODE`, `/reel/CODE`, `/tv/CODE`), a shortcode or the
//! numeric media id. The shortcode is the media id in Instagram's base64
//! alphabet, so the two convert both ways without a request.

use atp_core::{Error, Result};
use url::Url;

const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// The numeric media id of a post argument.
pub fn media_id(arg: &str) -> Result<String> {
  let a = arg.trim();
  // `MEDIAID_USERID` as the API writes ids.
  let a = a.split('_').next().unwrap_or(a);
  if !a.is_empty() && a.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(a.to_owned());
  }
  let code = shortcode(arg)?;
  decode(&code).ok_or_else(|| Error::input(format!("not an Instagram post: {arg}")))
}

/// The shortcode of a post link or a bare shortcode.
pub fn shortcode(arg: &str) -> Result<String> {
  let a = arg.trim();
  if let Ok(u) = Url::parse(a) {
    let mut seg = u.path_segments().into_iter().flatten();
    while let Some(s) = seg.next() {
      if matches!(s, "p" | "reel" | "reels" | "tv")
        && let Some(code) = seg.next().filter(|c| !c.is_empty())
      {
        return Ok(code.to_owned());
      }
    }
    return Err(Error::input(format!("not an Instagram post link: {a}")));
  }
  if !a.is_empty() && a.bytes().all(|b| ALPHABET.contains(&b)) {
    return Ok(a.to_owned());
  }
  Err(Error::input(format!("not an Instagram post: {a}")))
}

pub fn decode(code: &str) -> Option<String> {
  // Private posts append more characters after the first 11.
  let code = if code.len() > 11 { &code[..11] } else { code };
  let mut n: u128 = 0;
  for b in code.bytes() {
    n = n.checked_mul(64)? + ALPHABET.iter().position(|&a| a == b)? as u128;
  }
  Some(n.to_string())
}

pub fn encode(id: &str) -> Option<String> {
  let mut n: u128 = id.split('_').next()?.parse().ok()?;
  let mut out = Vec::new();
  while n > 0 {
    out.push(ALPHABET[(n % 64) as usize]);
    n /= 64;
  }
  out.reverse();
  String::from_utf8(out).ok()
}

pub fn post_url(code: &str) -> String {
  format!("https://www.instagram.com/p/{code}/")
}

/// A username (`name`, `@name`, a profile link) or `None` for a numeric id.
pub fn username(arg: &str) -> Option<String> {
  let a = arg.trim();
  if let Ok(u) = Url::parse(a) {
    return u
      .path_segments()
      .and_then(|mut s| s.next())
      .filter(|s| !s.is_empty())
      .map(str::to_owned);
  }
  let a = a.trim_start_matches('@');
  (!a.bytes().all(|b| b.is_ascii_digit())).then(|| a.to_owned())
}

pub fn tag(arg: &str) -> String {
  let a = arg.trim();
  if let Ok(u) = Url::parse(a)
    && let Some(t) = u
      .path_segments()
      .and_then(|s| s.skip_while(|x| *x != "tags").nth(1))
  {
    return t.to_owned();
  }
  a.trim_start_matches('#').to_owned()
}

pub fn location(arg: &str) -> String {
  let a = arg.trim();
  if let Ok(u) = Url::parse(a)
    && let Some(id) = u
      .path_segments()
      .and_then(|s| s.skip_while(|x| *x != "locations").nth(1))
  {
    return id.to_owned();
  }
  a.to_owned()
}
