//! The Web Service digital signature: `sig` is the MD5 of the parameters
//! (key included, sig excluded) sorted by name and joined as `k=v&k=v`,
//! with the key's private signing secret appended. Values are signed raw,
//! before URL encoding.

use md5::{Digest, Md5};

pub fn sig(params: &[(String, String)], secret: &str) -> String {
  let mut sorted: Vec<&(String, String)> = params.iter().collect();
  sorted.sort_by(|a, b| a.0.cmp(&b.0));
  let joined = sorted
    .iter()
    .map(|(k, v)| format!("{k}={v}"))
    .collect::<Vec<_>>()
    .join("&");
  format!("{:x}", Md5::digest(format!("{joined}{secret}").as_bytes()))
}
