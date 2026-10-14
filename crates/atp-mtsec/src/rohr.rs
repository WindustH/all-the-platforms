//! The rohr `_token` the sites' pages add to their requests: base64 of
//! zlib over a small JSON object. It names the business (`rId`), the
//! times, the browser window, the page (`bI`), the visitor's input tracks
//! and `sign`, which packs the sorted query the same way. atp sends empty
//! tracks: it does not make up mouse or key input.

use std::io::Write;

use atp_core::{Value, json};
use base64::Engine;
use flate2::Compression;
use flate2::write::ZlibEncoder;

const VERSION: &str = "1.0.6";
/// The window and screen of the desktop Chrome atp's client presents.
const VIEW: [u32; 2] = [1920, 947];
const SCREEN: [u32; 2] = [1920, 1080];

/// The `_token` for a request from `page` with `query` (which must not
/// hold `_token` itself).
pub fn token(rid: u32, page: &str, query: &[(String, String)]) -> String {
  let now = jiff::Timestamp::now().as_millisecond();
  let body = json!({
    "rId": rid.to_string(),
    "ver": VERSION,
    "ts": now,
    "cts": now,
    "brVD": VIEW,
    "brR": [SCREEN, [SCREEN[0], SCREEN[1] - 40], 24, 24],
    "bI": [page, page],
    "mT": [],
    "kT": [],
    "aT": [],
    "tT": [],
    "aM": "",
    "sign": sign(query),
  });
  pack(&body.to_string())
}

/// `sign`: the query sorted by name, `k=v&k=v`, packed as a JSON string.
pub fn sign(query: &[(String, String)]) -> String {
  let mut pairs: Vec<&(String, String)> = query.iter().collect();
  pairs.sort();
  let text = pairs
    .iter()
    .map(|(k, v)| format!("{k}={v}"))
    .collect::<Vec<_>>()
    .join("&");
  pack(&Value::String(text).to_string())
}

/// Base64 of zlib over `text`.
pub fn pack(text: &str) -> String {
  let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
  // Writing to memory does not fail.
  let _ = zlib.write_all(text.as_bytes());
  let bytes = zlib.finish().unwrap_or_default();
  base64::engine::general_purpose::STANDARD.encode(bytes)
}
