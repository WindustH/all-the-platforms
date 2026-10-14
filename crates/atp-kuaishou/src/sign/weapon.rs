//! The webweapon `kwfv1` value (cookie, and the `kww` header): the SDK's
//! first-visit form, AES-128-CBC (IV = key) over
//! `href|did|product|time|nonce`, base64, wrapped as `K....W...F..`.
//! Browsers that ran the SDK's VM carry a longer form; a session imported
//! from one keeps it.

use aes::Aes128;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockEncryptMut, KeyIvInit};
use rand::Rng;

use super::encode_uri;

/// `kwpsecproductname` of each site.
pub const PRODUCT_WWW: &str = "kuaishou-vision";
pub const PRODUCT_LIVE: &str = "PCLive";
pub const PRODUCT_CP: &str = "onvideo-cp";

const FINGERPRINT_KEY: &[u8; 16] = b"K8wm5PvY9nX7qJc2";

/// `kwfv1` for a page (`href`) of a site (`product`).
pub fn kwfv1(href: &str, did: &str, product: &str, now_ms: u64) -> String {
  let href: String = href.chars().take(80).collect();
  let nonce: String = rand::rng()
    .sample_iter(rand::distr::Alphanumeric)
    .take(8)
    .map(char::from)
    .collect();
  let plain = format!("{}|{did}|{product}|{now_ms}|{nonce}", encode_uri(&href));
  wrap(&encrypt(plain.as_bytes(), FINGERPRINT_KEY), 'F')
}

fn encrypt(data: &[u8], key: &[u8; 16]) -> String {
  let mut buf = data.to_vec();
  buf.resize(data.len() + 16, 0);
  let len = cbc::Encryptor::<Aes128>::new(key.into(), key.into())
    .encrypt_padded_mut::<Pkcs7>(&mut buf, data.len())
    .expect("buffer has room for padding")
    .len();
  buf.truncate(len);
  STANDARD.encode(buf)
}

/// `K` + 4 chars + `W` + the middle + tag + the last 2 chars.
fn wrap(s: &str, tag: char) -> String {
  let (head, rest) = s.split_at(4);
  let (middle, tail) = rest.split_at(rest.len() - 2);
  format!("K{head}W{middle}{tag}{tail}")
}
