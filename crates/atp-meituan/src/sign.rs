//! The 美团联盟 API's request signature, as its SDKs make it: HMAC-SHA256
//! with the app secret over
//! `POST\n<Content-MD5>\nS-Ca-App:<app key>\nS-Ca-Timestamp:<ms>\n<path>`,
//! base64. `Content-MD5` is the base64 MD5 of the exact body sent.

use base64::Engine;
use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use sha2::Sha256;

/// The headers the signature covers, as `S-Ca-Signature-Headers` names
/// them.
pub const SIGNED_HEADERS: &str = "S-Ca-App,S-Ca-Timestamp";

pub fn content_md5(body: &[u8]) -> String {
  base64::engine::general_purpose::STANDARD.encode(Md5::digest(body))
}

pub fn signature(
  secret: &str,
  content_md5: &str,
  app_key: &str,
  timestamp: &str,
  path: &str,
) -> String {
  let text = format!("POST\n{content_md5}\nS-Ca-App:{app_key}\nS-Ca-Timestamp:{timestamp}\n{path}");
  let mut mac =
    Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes any key length");
  mac.update(text.as_bytes());
  base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}
