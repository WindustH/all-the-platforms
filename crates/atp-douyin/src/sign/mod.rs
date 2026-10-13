//! Request signing of Douyin web.
//!
//! - [`abogus`]: the `a_bogus` query parameter every `/aweme/v1/web/` call
//!   carries (it covers the query, the body and the User-Agent).
//! - [`web_sign`]: `x-secsdk-web-signature` (secsdk `webSignUrl`), added on
//!   the paths the SDK protects when the visitor has a `UIFID`
//!   (`Evil0ctal/Douyin_TikTok_Download_API`, `websign.py`, Apache-2.0).
//! - [`verify_fp`]: the `s_v_web_id` / `verifyFp` visitor fingerprint.

pub mod abogus;

use md5::{Digest, Md5};
use rand::Rng;

/// Paths the web SDK's `webSign` strategy protects (its
/// `protectedHost["www.douyin.com"]` list).
const PROTECTED: &[&str] = &[
  "/aweme/v1/web/aweme/detail/",
  "/aweme/v1/web/aweme/post/",
  "/aweme/v1/web/aweme/favorite/",
  "/aweme/v1/web/aweme/listcollection/",
  "/aweme/v1/web/mix/aweme/",
  "/aweme/v1/web/tab/feed/",
  "/aweme/v1/web/mix/list/",
  "/aweme/v1/web/music/aweme/",
  "/aweme/v1/web/music/list/",
  "/aweme/v1/web/mix/detail/",
  "/aweme/v1/web/mix/listcollection/",
  "/aweme/v1/web/music/detail/",
  "/aweme/v1/web/collects/list/",
  "/aweme/v1/web/collects/video/list/",
];

const WEB_SIGN_SALT: &str = "A96D855A08C0A9707F8BEF0D9A527E4E";

pub fn is_protected(path: &str) -> bool {
  PROTECTED.contains(&path)
}

/// Percent-encoding of the query as both signatures see it: everything but
/// ASCII alphanumerics and `*-._` (Python's `quote(v, safe="*-._")`).
pub fn encode(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for b in s.bytes() {
    if b.is_ascii_alphanumeric() || matches!(b, b'*' | b'-' | b'.' | b'_') {
      out.push(char::from(b));
    } else {
      out.push_str(&format!("%{b:02X}"));
    }
  }
  out
}

pub fn query_string(pairs: &[(String, String)]) -> String {
  pairs
    .iter()
    .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
    .collect::<Vec<_>>()
    .join("&")
}

/// `query` (already ending in `a_bogus`) with `timestamp` and
/// `x-secsdk-web-signature` appended; `uifid` must be among its parameters.
pub fn web_sign(query: &str, uifid: &str, now_secs: i64) -> String {
  let hashed = format!("{query}&timestamp={now_secs}");
  let digest = Md5::digest(format!("{uifid}_{now_secs}_{WEB_SIGN_SALT}_{hashed}").as_bytes());
  let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
  format!("{hashed}&x-secsdk-web-signature={hex}")
}

/// `verify_<ms base36>_<uuid-like>`, as the web client mints `s_v_web_id`.
pub fn verify_fp(now_ms: i64) -> String {
  const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
  let mut ms = now_ms.max(0) as u64;
  let mut base36 = Vec::new();
  while ms > 0 {
    let d = (ms % 36) as u8;
    base36.push(if d < 10 { b'0' + d } else { b'a' + d - 10 });
    ms /= 36;
  }
  base36.reverse();
  let mut rng = rand::rng();
  let mut tail = [0u8; 36];
  for (i, slot) in tail.iter_mut().enumerate() {
    *slot = match i {
      8 | 13 | 18 | 23 => b'_',
      14 => b'4',
      _ => {
        let r = rng.random_range(0..DIGITS.len());
        DIGITS[if i == 19 { (r & 3) | 8 } else { r }]
      }
    };
  }
  format!(
    "verify_{}_{}",
    String::from_utf8_lossy(&base36),
    String::from_utf8_lossy(&tail)
  )
}
