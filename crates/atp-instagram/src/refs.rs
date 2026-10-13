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
  let trimmed = arg.trim();
  // `MEDIAID_USERID` as the API writes ids.
  let id = trimmed.split('_').next().unwrap_or(trimmed);
  if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(id.to_owned());
  }
  let code = shortcode(arg)?;
  decode(&code).ok_or_else(|| Error::input(format!("not an Instagram post: {arg}")))
}

/// The shortcode of a post link or a bare shortcode.
pub fn shortcode(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if let Ok(url) = Url::parse(arg) {
    let mut segments = url.path_segments().into_iter().flatten();
    while let Some(segment) = segments.next() {
      if matches!(segment, "p" | "reel" | "reels" | "tv")
        && let Some(code) = segments.next().filter(|c| !c.is_empty())
      {
        return Ok(code.to_owned());
      }
    }
    return Err(Error::input(format!("not an Instagram post link: {arg}")));
  }
  if !arg.is_empty() && arg.bytes().all(|b| ALPHABET.contains(&b)) {
    return Ok(arg.to_owned());
  }
  Err(Error::input(format!("not an Instagram post: {arg}")))
}

/// The media id a shortcode encodes.
pub fn decode(code: &str) -> Option<String> {
  // Private posts append more characters after the first 11.
  let code = if code.len() > 11 { &code[..11] } else { code };
  let mut n: u128 = 0;
  for b in code.bytes() {
    n = n.checked_mul(64)? + ALPHABET.iter().position(|&c| c == b)? as u128;
  }
  Some(n.to_string())
}

/// The shortcode of a media id.
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
  let arg = arg.trim();
  if let Ok(url) = Url::parse(arg) {
    return url
      .path_segments()
      .and_then(|mut segments| segments.next())
      .filter(|name| !name.is_empty())
      .map(str::to_owned);
  }
  let name = arg.trim_start_matches('@');
  (!name.bytes().all(|b| b.is_ascii_digit())).then(|| name.to_owned())
}

/// A hashtag (`tag`, `#tag`, an /explore/tags/ link).
pub fn tag(arg: &str) -> String {
  let arg = arg.trim();
  after(arg, "tags").unwrap_or_else(|| arg.trim_start_matches('#').to_owned())
}

/// A place id (the id, an /explore/locations/ link).
pub fn location(arg: &str) -> String {
  let arg = arg.trim();
  after(arg, "locations").unwrap_or_else(|| arg.to_owned())
}

/// The path segment after `segment` of a link.
fn after(arg: &str, segment: &str) -> Option<String> {
  Url::parse(arg)
    .ok()?
    .path_segments()?
    .skip_while(|s| *s != segment)
    .nth(1)
    .map(str::to_owned)
}
