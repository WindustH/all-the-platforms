//! Request signing of the TikTok web client: an `msToken` query parameter and
//! the `X-Bogus` signature over the final query string and the User-Agent.
//!
//! X-Bogus is the public algorithm (as in the Douyin / TikTok download
//! tools): double MD5 of the query and of the empty body, MD5 of the
//! RC4-encrypted User-Agent, a timestamp and a fixed canvas value, XOR
//! checksummed, RC4-encrypted again and mapped through a custom alphabet.
//! Newer pages also send `X-Gnarly`, which is not reproduced here.
//!
//! The live rooms' websocket carries a `frontierSign` instead
//! ([`webcast_signature`]).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use md5::{Digest, Md5};
use rand::Rng;

const ALPHABET: &[u8; 65] = b"Dkdpgh4ZKsQB80/Mfvw36XI1R25-WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe=";
const UA_KEY: [u8; 3] = [0, 1, 12];
const CANVAS: u32 = 536_919_696;
const TOKEN_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// The `X-Bogus` of the webcast websocket. The page signs an empty
/// `X-MS-PAYLOAD` where Douyin signs an `X-MS-STUB`, so the stub is the MD5
/// of nothing. TikTok accepts the connection without checking it today.
pub fn webcast_signature() -> String {
  atp_bytedance::webcast::frontier::sign(&md5(b""))
}

/// A fresh `msToken` for a session the server has not handed one yet.
pub fn ms_token() -> String {
  let mut rng = rand::rng();
  (0..128)
    .map(|_| TOKEN_CHARS[rng.random_range(0..TOKEN_CHARS.len())] as char)
    .collect()
}

/// `X-Bogus` of `query` (exactly as sent, without `X-Bogus`) at `now` (unix seconds).
pub fn x_bogus(query: &str, user_agent: &str, now: u32) -> String {
  let q = md5(&md5(query.as_bytes()));
  let body = md5(&md5(b""));
  let ua = md5(
    STANDARD
      .encode(rc4(&UA_KEY, user_agent.as_bytes()))
      .as_bytes(),
  );
  let mut arr = vec![
    64u8, 0, 1, 12, q[14], q[15], body[14], body[15], ua[14], ua[15],
  ];
  arr.extend(now.to_be_bytes());
  arr.extend(CANVAS.to_be_bytes());
  let checksum = arr.iter().fold(0u8, |x, b| x ^ b);
  arr.push(checksum);
  let mut data = vec![2u8, 255];
  data.extend(rc4(&[255], &arr));
  encode(&data)
}

fn md5(data: &[u8]) -> [u8; 16] {
  Md5::digest(data).into()
}

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
  let mut s: Vec<u8> = (0..=255).collect();
  let mut j = 0usize;
  for i in 0..256 {
    j = (j + s[i] as usize + key[i % key.len()] as usize) % 256;
    s.swap(i, j);
  }
  let (mut i, mut j) = (0usize, 0usize);
  data
    .iter()
    .map(|b| {
      i = (i + 1) % 256;
      j = (j + s[i] as usize) % 256;
      s.swap(i, j);
      b ^ s[(s[i] as usize + s[j] as usize) % 256]
    })
    .collect()
}

/// Base64 packing through the client's alphabet (input is a multiple of 3).
fn encode(data: &[u8]) -> String {
  data
    .chunks(3)
    .flat_map(|c| {
      let n = (c[0] as u32) << 16
        | (*c.get(1).unwrap_or(&0) as u32) << 8
        | *c.get(2).unwrap_or(&0) as u32;
      [18, 12, 6, 0].map(|shift| ALPHABET[(n >> shift & 63) as usize] as char)
    })
    .collect()
}
