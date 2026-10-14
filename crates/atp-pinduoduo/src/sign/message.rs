//! `getMessage` (program 3 of the crawler's VM): the collectors' fields in
//! their fixed order, framed with a version and a length, deflated (zlib,
//! as `pako.deflate`), followed by the code checksum and encoded as an
//! `0as` token. Touch, click and scroll collectors stay empty: atp sends
//! no input events.

use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use rand::Rng;

use super::device::{BOT_BITS, BROWSER_BITS, CODE_SUM, LOCATION_SUM, MEDIA, SCREEN, USER_AGENT};
use super::encode::{self, Value, checksum, field, varint};

/// The page state a token describes.
pub struct Inputs<'a> {
  /// `location.href` when the page loaded.
  pub href: &'a str,
  /// `document.referrer` (its query is dropped).
  pub referrer: &'a str,
  /// Cookies `pdd_user_id`, `api_uid` and `pdd_vds`, when set.
  pub user_id: Option<String>,
  pub api_uid: Option<String>,
  pub vds: Option<String>,
  /// Tokens made on this page so far, this one included.
  pub count: u64,
  /// When the page loaded (ms).
  pub loaded: i64,
  /// The server time the page was given (ms).
  pub server_time: i64,
  /// Milliseconds since the server time was set.
  pub since_sync: u64,
  pub nano_fp: &'a str,
  /// `anti_pre_sig` entries.
  pub tails: &'a [String],
}

/// The collectors' fields, in `getMessage`'s order.
pub fn body(inputs: &Inputs) -> Vec<u8> {
  let mut out = Vec::new();
  out.extend(location(inputs.href));
  out.extend(field(8, Value::None));
  out.extend(varint(SCREEN.0));
  out.extend(varint(SCREEN.1));
  out.extend(field(9, Value::Text(&updated_token(inputs.server_time))));
  if !inputs.tails.is_empty() {
    out.extend(field(29, Value::Text(&inputs.tails.join(","))));
  }
  out.extend(field(10, Value::None));
  out.extend(varint(BOT_BITS));
  out.extend(field(11, Value::None));
  out.extend(checksum(inputs.href));
  out.extend(field(12, Value::Text("y")));
  out.extend(field(13, Value::Text("y")));
  out.extend(field(14, Value::Number(inputs.since_sync)));
  out.extend(field(15, Value::Text(USER_AGENT)));
  out.extend(field(16, Value::Text(inputs.nano_fp)));
  out.extend(field(17, Value::Text(inputs.nano_fp)));
  out.extend(cookies(inputs));
  out.extend(field(26, Value::None));
  out.extend(varint(BROWSER_BITS));
  out.extend(field(27, Value::None));
  out.extend(varint(1));
  out.extend(field(28, Value::None));
  out.extend(varint(MEDIA.0));
  out.extend(varint(MEDIA.1));
  out
}

/// `LocationInfo`: the page link (128 characters at most), an empty port
/// and the collector's code checksum.
fn location(href: &str) -> Vec<u8> {
  let href: String = href.chars().take(128).collect();
  let bytes = encode::text(&href);
  let mut out = field(7, Value::None);
  out.extend(varint(bytes.len() as u64));
  out.extend(bytes);
  out.extend(varint(0));
  out.extend(LOCATION_SUM);
  out
}

/// `UpdatedToken`: two random integers added up, then the server time.
fn updated_token(server_time: i64) -> String {
  let mut rng = rand::rng();
  let a: u64 = rng.random_range(0..=1u64 << 52);
  let b: u64 = rng.random_range(0..=1u64 << 30);
  format!("{}-{server_time}", a + b)
}

/// Referrer, the cookie fields, the call count and the load time.
fn cookies(inputs: &Inputs) -> Vec<u8> {
  let mut out = Vec::new();
  let referrer = inputs.referrer.split('?').next().unwrap_or_default();
  let text = [
    (18, Some(referrer.to_owned())),
    (19, inputs.user_id.clone()),
    (20, inputs.api_uid.clone()),
  ];
  for (kind, value) in text {
    if let Some(value) = value.filter(|v| !v.is_empty()) {
      out.extend(field(kind, Value::Text(&value)));
    }
  }
  out.extend(field(21, Value::Number(inputs.count)));
  out.extend(field(22, Value::Number(inputs.loaded.max(0) as u64)));
  if let Some(vds) = inputs.vds.as_deref().filter(|v| !v.is_empty()) {
    out.extend(field(23, Value::Text(vds)));
  }
  out
}

/// The token for a body: version 2 (`antiVersion` v1), `[1, 0, 0]`, the
/// body length in two bytes, the body; deflated, the code checksum
/// appended, encoded with a random offset and prefixed `0as`.
pub fn token(body: &[u8]) -> String {
  let len = body.len();
  let mut framed = vec![2, 1, 0, 0];
  framed.extend(match len {
    0 => [0, 0],
    1..=255 => [0, len as u8],
    _ => [(len >> 8) as u8, len as u8],
  });
  framed.extend_from_slice(body);
  let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
  let mut data: Vec<u8> = zlib
    .write_all(&framed)
    .and_then(|_| zlib.finish())
    .unwrap_or_default();
  data.extend(CODE_SUM);
  let offset = rand::rng().random_range(0..64);
  format!("0as{}", encode::encode(&data, Some(offset)))
}
