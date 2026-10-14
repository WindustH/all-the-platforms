//! The request encodings of NetEase Cloud Music, pure computation:
//!
//! - `weapi` (the web player): the JSON body AES-128-CBC encrypted twice,
//!   first with the preset key, then with a random 16-character secret,
//!   both base64; the secret goes along reversed and RSA-encrypted with the
//!   site's public key (`params` + `encSecKey`).
//! - `eapi` (the desktop client): `path-36cd479b6b5-json-36cd479b6b5-md5`
//!   AES-128-ECB encrypted with the eapi key, as upper-case hex (`params`).

mod cipher;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use md5::{Digest, Md5};
use rand::Rng;

/// The preset key and IV of the first CBC round (and the IV of the second).
const PRESET_KEY: &[u8; 16] = b"0CoJUm6Qyw8W8jud";
const IV: &[u8; 16] = b"0102030405060708";
const EAPI_KEY: &[u8; 16] = b"e82ckenh8dichen8";
const SECRET_CHARS: &[u8; 62] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
/// The site's RSA public key: exponent 65537 and this 1024-bit modulus.
const EXPONENT: u32 = 0x10001;
const MODULUS: &str = "00e0b509f6259df8642dbc35662901477df22677ec152b5ff68ace615bb7b725152b3ab17a876aea8a5aa76d2e417629ec4ee341f56135fccf695280104e0312ecbda92557c93870114af6c9d05c4f7f0c3685b7a46bee255932575cce10b424d813cfe4875d3e82047b97ddef52741d546b8e289dc6935b3ece0462db0a22b8e7";
/// What separates the parts of an eapi message.
const EAPI_GLUE: &str = "-36cd479b6b5-";

/// The form fields of a weapi request for the JSON `text`.
pub fn weapi(text: &str) -> [(&'static str, String); 2] {
  let mut rng = rand::rng();
  let secret: String = (0..16)
    .map(|_| SECRET_CHARS[rng.random_range(0..SECRET_CHARS.len())] as char)
    .collect();
  weapi_with(text, &secret)
}

/// [`weapi`] with a given secret (16 ASCII characters).
pub fn weapi_with(text: &str, secret: &str) -> [(&'static str, String); 2] {
  let key: &[u8; 16] = secret.as_bytes().try_into().expect("a 16-byte secret");
  let once = base64(&cipher::cbc(PRESET_KEY, IV, text.as_bytes()));
  let params = base64(&cipher::cbc(key, IV, once.as_bytes()));
  let reversed: Vec<u8> = secret.bytes().rev().collect();
  let enc_sec_key = cipher::rsa(&reversed, EXPONENT, MODULUS);
  [("params", params), ("encSecKey", enc_sec_key)]
}

/// The `params` of an eapi request: `path` is the `/api/...` path the
/// request stands for, `text` its JSON body.
pub fn eapi(path: &str, text: &str) -> String {
  let digest = cipher::hex(&Md5::digest(
    format!("nobody{path}use{text}md5forencrypt").as_bytes(),
  ));
  let message = format!("{path}{EAPI_GLUE}{text}{EAPI_GLUE}{digest}");
  cipher::hex(&cipher::ecb(EAPI_KEY, message.as_bytes())).to_ascii_uppercase()
}

fn base64(data: &[u8]) -> String {
  STANDARD.encode(data)
}
