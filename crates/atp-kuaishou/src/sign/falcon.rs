//! `__NS_hxfalcon` (sig4): a device blob (`HUDR_…`, ChaCha20 over a small
//! TLV record) followed by `$HE_` and a xor container. The container holds
//! the SDK version, the signer's start time, a random 48-bit number, the
//! call counter, four bytes of a BLAKE2s variant of the request run through
//! an LFSR stream, and the time.

use std::cell::Cell;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::{checksum, hex, now_ms};

/// SDK version of the www and login pages.
pub const SDK_WWW: u64 = 43468;
/// SDK version of the live site.
pub const SDK_LIVE: u64 = 43469;

const SIGMA: [[usize; 16]; 10] = [
  [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
  [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
  [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
  [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
  [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
  [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
  [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
  [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
  [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
  [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];
const BLAKE_IV: [u32; 8] = [
  2837534710, 2845986804, 2436420605, 706843635, 719254516, 2557931286, 2596197199, 2432949778,
];
const BLAKE_PARAM: u32 = 16842784;

const EO: [u8; 48] = [
  98, 0, 0, 128, 49, 117, 185, 253, 224, 172, 104, 36, 223, 155, 87, 19, 32, 0, 0, 64, 2, 0, 0, 16,
  255, 255, 255, 127, 255, 255, 255, 63, 0, 0, 0, 240, 0, 0, 0, 192, 0, 0, 0, 128, 255, 255, 255,
  15,
];
const CTS_KEY: &[u8] = b"Vuz4fCHxn1CO";

const CHACHA_CONST: [u32; 4] = [394484062, 2378328696, 630790222, 1922531795];
const CHACHA_KEY: [u32; 8] = [
  4183807412, 394484062, 1106561997, 2378328696, 630790222, 2546784104, 2891127470, 1922531795,
];
const CHACHA_NONCE: [u32; 3] = [2215853858, 1643070585, 1849059804];
const BLOB_XOR: u8 = 35;
const ENV: [u8; 7] = [0xe0, 0, 0, 0, 0, 0, 0];
const XOR_DIGEST: [u8; 4] = [45, 211, 69, 192];
const XOR_ENV: [u8; 4] = [123, 86, 62, 218];
const COUNT_MASK: u64 = 3131873467;
const NOW_MASK: u64 = 3360347992;
const RAND_SPAN: f64 = 281474976710655.0;
const SCRIPTS_LEN: u64 = 24;
const SECS_STACK: &str = "spatchRequest (https://p23-plat.wskwai.com/kos/nlav111422/ks-web/assets/index-bZyTA7JL.js:32:235479)";

/// One page session's signer: its start time and two counters from 100.
pub struct Falcon {
  sdk: u64,
  startup: u64,
  count: Cell<u64>,
  guard: Cell<u64>,
}

impl Falcon {
  pub fn new(sdk: u64) -> Self {
    Self {
      sdk,
      startup: now_ms(),
      count: Cell::new(100),
      guard: Cell::new(100),
    }
  }

  /// Sign the serialized request (see [`super::falcon_input`]).
  pub fn sign(&self, input: &str) -> String {
    let rand48 = (rand::random::<f64>() * RAND_SPAN) as u64;
    self.encode(input, now_ms(), rand48)
  }

  fn encode(&self, input: &str, now: u64, rand48: u64) -> String {
    let count = self.count.get();
    let guard = self.guard.get();
    let prefix = device_prefix(guard, count);
    self.guard.set(guard + 1);
    let hash = blake_hex(&format!("{input}{prefix}"));
    let stream = Cts::new().apply(&hash.as_bytes()[..4]);
    let env: Vec<u8> = ENV
      .iter()
      .enumerate()
      .map(|(i, b)| b ^ XOR_ENV[i % 4])
      .collect();
    let mut body = vec![0x4b, 0x54];
    body.extend(le(self.sdk, 2));
    body.push(0xab);
    body.extend(le(self.startup, 6));
    body.extend(le(rand48, 6));
    body.extend([1, 0, 0, 0, 1]);
    body.extend(le(count ^ COUNT_MASK, 4));
    body.extend((0..4).map(|i| stream[i] ^ XOR_DIGEST[i]));
    body.extend(le(now ^ NOW_MASK, 6));
    let env_sum = checksum(&env);
    body.extend(env);
    body.push(env_sum);
    self.count.set(count + 1);
    format!("{prefix}$HE_{}", hex(&container(body)))
  }
}

/// Little-endian bytes of `value`.
fn le(value: u64, n: usize) -> impl Iterator<Item = u8> {
  (0..n).map(move |i| (value >> (8 * i)) as u8)
}

/// The body with its checksum appended, every other byte xored with it.
fn container(mut body: Vec<u8>) -> Vec<u8> {
  let key = checksum(&body);
  body.iter_mut().for_each(|b| *b ^= key);
  body.push(key);
  body
}

/// `collectDeviceInfo()`: a TLV record xored with 35, ChaCha20, url-safe
/// base64 with `.` padding.
fn device_prefix(guard: u64, count: u64) -> String {
  let mut blob = vec![45, 61, 0, 2, 68, 0];
  blob.extend(le(SCRIPTS_LEN, 4));
  blob.extend([112, 0]);
  blob.extend(le(guard, 4));
  blob.extend([114, 1]);
  blob.extend(le(SECS_STACK.len() as u64, 2));
  blob.extend(SECS_STACK.bytes());
  blob.extend([115, 0]);
  blob.extend(le(count, 4));
  let xored: Vec<u8> = blob.iter().map(|b| b ^ BLOB_XOR).collect();
  let encoded = STANDARD
    .encode(chacha(&xored))
    .replace('+', "-")
    .replace('/', "_")
    .replace('=', ".");
  format!("HUDR_{encoded}")
}

fn blake_g(v: &mut [u32; 16], (a, b, c, d): (usize, usize, usize, usize), x: u32, y: u32) {
  v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
  v[d] = (v[d] ^ v[a]).rotate_right(16);
  v[c] = v[c].wrapping_add(v[d]);
  v[b] = (v[b] ^ v[c]).rotate_right(12);
  v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
  v[d] = (v[d] ^ v[a]).rotate_right(8);
  v[c] = v[c].wrapping_add(v[d]);
  v[b] = (v[b] ^ v[c]).rotate_right(7);
}

fn blake_compress(h: &mut [u32; 8], words: &[u32], counter: u32, last: bool) {
  let mut v = [0u32; 16];
  v[..8].copy_from_slice(h);
  v[8..].copy_from_slice(&BLAKE_IV);
  v[12] ^= counter;
  if last {
    v[14] ^= 0xffff_ffff;
  }
  let mut m = [0u32; 16];
  for (i, w) in words.iter().enumerate() {
    m[i % 16] ^= w;
  }
  for s in SIGMA {
    blake_g(&mut v, (0, 4, 8, 12), m[s[0]], m[s[1]]);
    blake_g(&mut v, (1, 5, 9, 13), m[s[2]], m[s[3]]);
    blake_g(&mut v, (2, 6, 10, 14), m[s[4]], m[s[5]]);
    blake_g(&mut v, (3, 7, 11, 15), m[s[6]], m[s[7]]);
    blake_g(&mut v, (0, 5, 10, 15), m[s[8]], m[s[9]]);
    blake_g(&mut v, (1, 6, 11, 12), m[s[10]], m[s[11]]);
    blake_g(&mut v, (2, 7, 8, 13), m[s[12]], m[s[13]]);
    blake_g(&mut v, (3, 4, 9, 14), m[s[14]], m[s[15]]);
  }
  for i in 0..8 {
    h[i] ^= v[i] ^ v[i + 8];
  }
}

/// The SDK's BLAKE2s variant (own IV, little-endian words, 64-word
/// blocks folded into 16), as 64 hex digits.
fn blake_hex(text: &str) -> String {
  let mut bytes = text.as_bytes().to_vec();
  bytes.resize(bytes.len().div_ceil(4) * 4, 0);
  let words: Vec<u32> = bytes
    .chunks(4)
    .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
    .collect();
  let mut h = BLAKE_IV;
  h[0] ^= BLAKE_PARAM;
  let mut rest = words.as_slice();
  let mut counter = 0u32;
  while rest.len() > 64 {
    counter += 64;
    blake_compress(&mut h, &rest[..64], counter, false);
    rest = &rest[64..];
  }
  counter += rest.len() as u32;
  blake_compress(&mut h, rest, counter, true);
  h.iter().map(|w| format!("{w:08x}")).collect()
}

/// Three LFSRs combined into a byte stream (`jmpOnw_cts`).
struct Cts {
  r: [u32; 3],
}

fn eo(at: usize) -> u32 {
  u32::from_le_bytes([EO[at], EO[at + 1], EO[at + 2], EO[at + 3]])
}

impl Cts {
  fn new() -> Self {
    let mut r = [eo(12), eo(8), eo(4)];
    for &byte in &CTS_KEY[4..8] {
      for reg in &mut r {
        *reg = (*reg << 8) | u32::from(byte);
      }
    }
    let fallback = [324508639, 610839776, 4256789809];
    for (reg, alt) in r.iter_mut().zip(fallback) {
      if *reg == 0 {
        *reg = alt;
      }
    }
    Self { r }
  }

  fn next(&mut self) -> u8 {
    let (p0, f1, f2) = (eo(0), eo(16), eo(20));
    let (m0, m1, m2) = (eo(24), eo(28), eo(44));
    let (h0, h1, h2) = (eo(40), eo(36), eo(32));
    let mut out = 0u8;
    let mut b0 = self.r[1] & 1;
    let mut b1 = self.r[2] & 1;
    for _ in 0..8 {
      let [r0, r1, r2] = &mut self.r;
      if *r0 & 1 == 1 {
        *r0 = (*r0 ^ (p0 >> 1)) | h0;
        b0 = *r1 & 1;
        *r1 = if b0 == 1 {
          (*r1 ^ (f1 >> 1)) | h1
        } else {
          (*r1 >> 1) & m1
        };
      } else {
        *r0 = (*r0 >> 1) & m0;
        b1 = *r2 & 1;
        *r2 = if b1 == 1 {
          (*r2 ^ (f2 >> 1)) | h2
        } else {
          (*r2 >> 1) & m2
        };
      }
      out = (out << 1) | (b0 ^ b1) as u8;
    }
    out
  }

  fn apply(mut self, data: &[u8]) -> Vec<u8> {
    data
      .iter()
      .map(|b| b ^ self.next().wrapping_add(3))
      .collect()
  }
}

fn chacha_block(state: &[u32; 16]) -> [u32; 16] {
  let mut w = *state;
  let mut q = |a: usize, b: usize, c: usize, d: usize| {
    w[a] = w[a].wrapping_add(w[b]);
    w[d] = (w[d] ^ w[a]).rotate_left(16);
    w[c] = w[c].wrapping_add(w[d]);
    w[b] = (w[b] ^ w[c]).rotate_left(12);
    w[a] = w[a].wrapping_add(w[b]);
    w[d] = (w[d] ^ w[a]).rotate_left(8);
    w[c] = w[c].wrapping_add(w[d]);
    w[b] = (w[b] ^ w[c]).rotate_left(7);
  };
  for _ in 0..10 {
    q(0, 4, 8, 12);
    q(1, 5, 9, 13);
    q(2, 6, 10, 14);
    q(3, 7, 11, 15);
    q(0, 5, 10, 15);
    q(1, 6, 11, 12);
    q(2, 7, 8, 13);
    q(3, 4, 9, 14);
  }
  let mut out = [0u32; 16];
  for i in 0..16 {
    out[i] = w[i].wrapping_add(state[i]);
  }
  out
}

/// ChaCha20 with the SDK's fixed key and nonce, counter from 1.
fn chacha(data: &[u8]) -> Vec<u8> {
  let mut state = [0u32; 16];
  state[..4].copy_from_slice(&CHACHA_CONST);
  state[4..12].copy_from_slice(&CHACHA_KEY);
  state[12] = 1;
  state[13..].copy_from_slice(&CHACHA_NONCE);
  let mut out = Vec::with_capacity(data.len());
  for chunk in data.chunks(64) {
    let block = chacha_block(&state);
    for (k, byte) in chunk.iter().enumerate() {
      out.push(byte ^ (block[k >> 2] >> ((k & 3) * 8)) as u8);
    }
    state[12] = state[12].wrapping_add(1);
  }
  out
}
