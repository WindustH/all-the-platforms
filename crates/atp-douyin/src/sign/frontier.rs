//! The `signature` of the webcast websocket: the web SDK's `frontierSign`
//! over `X-MS-STUB`, the MD5 of a few connection parameters. Pure
//! computation, ported from `Johnserf-Seed/f2`
//! (`f2/apps/douyin/algorithm/webcast_signature.py`, Apache-2.0, webmssdk
//! 1.0.0.53): twelve bytes (flags, an RC4 key, then the RC4-sealed block of
//! counter, environment codes, MD5 tails and a checksum) in the SDK's
//! alphabet.

use std::sync::atomic::{AtomicU8, Ordering};

use md5::{Digest, Md5};
use rand::Rng;

/// The `webcast_sdk_version` the stub and the connection name.
pub const SDK_VERSION: &str = "1.0.14-beta.0";

const ALPHABET: &[u8; 64] = b"Dkdpgh4ZKsQB80/Mfvw36XI1R25+WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe";
/// `kWebsocket` set, `initialized` clear.
const FLAGS: u8 = 1 << 6;

static COUNTER: AtomicU8 = AtomicU8::new(0);

/// The signature for a connection to `room_id` as `user_unique_id`.
pub fn signature(room_id: &str, user_unique_id: &str) -> String {
  let stub = format!(
    "live_id=1,aid=6383,version_code=180800,webcast_sdk_version={SDK_VERSION},\
     room_id={room_id},sub_room_id=,sub_channel_id=,did_rule=3,\
     user_unique_id={user_unique_id},device_platform=web,device_type=,ac=,\
     identity=audience"
  );
  sign(&Md5::digest(stub.as_bytes()))
}

/// `frontierSign` of a stub digest: its bytes are what the SDK's hex
/// decoding of the stub yields.
fn sign(stub: &[u8]) -> String {
  let mut rng = rand::rng();
  let flag_random: u8 = rng.random_range(0..100);
  let tail_random: u8 = rng.random_range(0..255);
  let key: u8 = rng.random_range(0..255);
  let empty_body = Md5::digest(Md5::digest(b""));
  let stub_hash = Md5::digest(stub);
  let count = COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_add(1) & 63;
  let mut block = vec![
    count,
    0,
    0,
    0,
    empty_body[14],
    empty_body[15],
    stub_hash[14],
    stub_hash[15],
    tail_random,
  ];
  block.push(block.iter().fold(0, |x, b| x ^ b));
  let mut out = vec![FLAGS | (flag_random & 1) << 4, key];
  out.extend(rc4(key, &block));
  encode(&out)
}

fn rc4(key: u8, data: &[u8]) -> Vec<u8> {
  let mut s: Vec<u8> = (0..=255).collect();
  let mut j = 0u8;
  for i in 0..256 {
    j = j.wrapping_add(s[i]).wrapping_add(key);
    s.swap(i, usize::from(j));
  }
  let (mut i, mut j) = (0u8, 0u8);
  data
    .iter()
    .map(|b| {
      i = i.wrapping_add(1);
      j = j.wrapping_add(s[usize::from(i)]);
      s.swap(usize::from(i), usize::from(j));
      b ^ s[usize::from(s[usize::from(i)].wrapping_add(s[usize::from(j)]))]
    })
    .collect()
}

/// Base64 in the SDK's alphabet, `=` padded.
fn encode(data: &[u8]) -> String {
  let mut out = String::new();
  for chunk in data.chunks(3) {
    let n = chunk.len();
    let v = chunk
      .iter()
      .enumerate()
      .fold(0u32, |v, (i, b)| v | u32::from(*b) << (16 - 8 * i));
    let at = |shift: u32| char::from(ALPHABET[((v >> shift) & 63) as usize]);
    out.push(at(18));
    out.push(at(12));
    out.push(if n > 1 { at(6) } else { '=' });
    out.push(if n > 2 { at(0) } else { '=' });
  }
  out
}
