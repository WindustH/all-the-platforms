//! The SN signature of AKs in SN mode: the MD5 of
//! `form-encode(path + "?" + query + SK)`, where `query` is the exact
//! query string sent (parameters in the order sent, `sn` not among them);
//! `sn` then goes last. Values are percent-encoded once here, so the
//! string signed is byte for byte the string sent.

use md5::{Digest, Md5};

/// A value as sent: everything but `A-Z a-z 0-9 - _ .` as `%XX`.
pub fn encode(value: &str) -> String {
  let mut out = String::with_capacity(value.len());
  for b in value.bytes() {
    if b.is_ascii_alphanumeric() || b"-_.".contains(&b) {
      out.push(b as char);
    } else {
      out.push_str(&format!("%{b:02X}"));
    }
  }
  out
}

/// `k=v&k=v` in order, values encoded.
pub fn query(params: &[(String, String)]) -> String {
  params
    .iter()
    .map(|(k, v)| format!("{k}={}", encode(v)))
    .collect::<Vec<_>>()
    .join("&")
}

/// `sn` of a GET request to `path` with this query string.
pub fn sn(path: &str, query: &str, sk: &str) -> String {
  let whole = form_encode(&format!("{path}?{query}{sk}"));
  format!("{:x}", Md5::digest(whole.as_bytes()))
}

/// `application/x-www-form-urlencoded` the way PHP's `urlencode` and
/// Python's `quote_plus` write it.
fn form_encode(text: &str) -> String {
  let mut out = String::with_capacity(text.len() * 2);
  for b in text.bytes() {
    match b {
      b' ' => out.push('+'),
      b if b.is_ascii_alphanumeric() || b"-_.".contains(&b) => out.push(b as char),
      b => out.push_str(&format!("%{b:02X}")),
    }
  }
  out
}
