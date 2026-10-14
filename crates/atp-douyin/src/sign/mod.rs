//! Douyin's own request signatures. `a_bogus`, the query encoding, the
//! visitor fingerprint and the ticket-guard keys are shared with Toutiao
//! and live in atp-bytedance.
//!
//! [`web_sign`]: `x-secsdk-web-signature` (secsdk `webSignUrl`), added on
//! the paths the SDK protects when the visitor has a `UIFID`
//! (`Evil0ctal/Douyin_TikTok_Download_API`, `websign.py`, Apache-2.0).
//! [`guard`] and [`dtrait`] add what writes carry, [`frontier`] signs the
//! webcast websocket.

pub mod dtrait;
pub mod frontier;
pub mod guard;

use md5::{Digest, Md5};

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

/// `query` (already ending in `a_bogus`) with `timestamp` and
/// `x-secsdk-web-signature` appended; `uifid` must be among its parameters.
pub fn web_sign(query: &str, uifid: &str, now_secs: i64) -> String {
  let hashed = format!("{query}&timestamp={now_secs}");
  let digest = Md5::digest(format!("{uifid}_{now_secs}_{WEB_SIGN_SALT}_{hashed}").as_bytes());
  let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
  format!("{hashed}&x-secsdk-web-signature={hex}")
}
