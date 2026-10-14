//! The WebService signature (`sig`): the MD5 of the request path, `?`,
//! every parameter (key included) sorted by name and joined as `k=v&k=v`
//! with raw, unencoded values, then the secret key (SK).

use md5::{Digest, Md5};

pub fn sig(path: &str, params: &[(String, String)], sk: &str) -> String {
  let mut sorted: Vec<&(String, String)> = params.iter().collect();
  sorted.sort_by(|a, b| a.0.cmp(&b.0));
  let joined = sorted
    .iter()
    .map(|(k, v)| format!("{k}={v}"))
    .collect::<Vec<_>>()
    .join("&");
  format!(
    "{:x}",
    Md5::digest(format!("{path}?{joined}{sk}").as_bytes())
  )
}

/// The query string sent: parameters in order, values percent-encoded
/// (everything but `A-Z a-z 0-9 - _ . ~`).
pub fn query(params: &[(String, String)]) -> String {
  params
    .iter()
    .map(|(k, v)| format!("{k}={}", encode(v)))
    .collect::<Vec<_>>()
    .join("&")
}

fn encode(value: &str) -> String {
  let mut out = String::with_capacity(value.len());
  for b in value.bytes() {
    if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
      out.push(b as char);
    } else {
      out.push_str(&format!("%{b:02X}"));
    }
  }
  out
}
