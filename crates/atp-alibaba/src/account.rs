//! The account a session belongs to, as both sites' cookies name it: `unb`
//! holds the user id, `_nk_` / `tracknick` the nickname (URL-encoded, with
//! `\uXXXX` escapes inside).

use atp_core::{Ctx, Error, Result};

/// Cookies of a logged-in session on either site.
pub const LOGIN_COOKIES: &[&str] = &["unb", "cookie2"];

/// The logged-in user's id, or `not_authenticated`.
pub fn me(ctx: &Ctx) -> Result<String> {
  ctx.require_login(LOGIN_COOKIES)?;
  ctx
    .http
    .cookie("unb")
    .ok_or_else(|| Error::auth("not logged in").with_hint(ctx.login_hint()))
}

/// The nickname cookie exactly as stored (the Taobao chat sends it so).
pub fn raw_nick(ctx: &Ctx) -> Option<String> {
  ctx
    .http
    .cookie("_nk_")
    .or_else(|| ctx.http.cookie("tracknick"))
}

/// The nickname, decoded.
pub fn nick(ctx: &Ctx) -> Option<String> {
  raw_nick(ctx).map(|raw| decode_nick(&raw))
}

/// `%E9%97%B2` and `闲` escapes undone.
pub fn decode_nick(raw: &str) -> String {
  unescape(&percent_decode(raw))
}

fn percent_decode(s: &str) -> String {
  let bytes = s.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    let hex = bytes
      .get(i + 1..i + 3)
      .and_then(|h| std::str::from_utf8(h).ok())
      .and_then(|h| u8::from_str_radix(h, 16).ok());
    match (bytes[i], hex) {
      (b'%', Some(b)) => {
        out.push(b);
        i += 3;
      }
      (b, _) => {
        out.push(b);
        i += 1;
      }
    }
  }
  String::from_utf8_lossy(&out).into_owned()
}

fn unescape(s: &str) -> String {
  let mut units: Vec<u16> = Vec::with_capacity(s.len());
  let mut rest = s;
  while let Some(at) = rest.find("\\u") {
    units.extend(rest[..at].encode_utf16());
    let code = rest
      .get(at + 2..at + 6)
      .and_then(|h| u16::from_str_radix(h, 16).ok());
    match code {
      Some(unit) => {
        units.push(unit);
        rest = &rest[at + 6..];
      }
      None => {
        units.extend("\\u".encode_utf16());
        rest = &rest[at + 2..];
      }
    }
  }
  units.extend(rest.encode_utf16());
  String::from_utf16_lossy(&units)
}
