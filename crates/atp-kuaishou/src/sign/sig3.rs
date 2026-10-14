//! `__NS_sig3` of the creator center's web SDK (`cp.kuaishou.com`): MD5 of
//! the request (here always a JSON body without query), two SHA-256
//! compressions with the SDK's own IVs, four bytes of which go into a
//! 28-byte xor container with the signer's start time, counter and build
//! number. Only the creator center accepts it: the app's `__NS_sig3` is
//! another, native algorithm (see [`super`]).

use std::cell::Cell;

use md5::{Digest, Md5};

use super::{checksum, hex, now_ms};

const SHA256_K: [u32; 64] = [
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const IV_FIRST: [i32; 8] = [
  1201087869,
  728038316,
  -1247317401,
  -375217708,
  -1820116536,
  395408228,
  1482956210,
  -1904517706,
];
const IV_SECOND: [i32; 8] = [
  -932960537,
  -839864669,
  895983619,
  323220038,
  1908748190,
  778712444,
  -2022813415,
  -1089440689,
];
const BUILD: u32 = 1653548225;
const TAIL: [u8; 7] = [1, 0, 1, 0, 0, 0, 0];

pub struct Sig3 {
  startup: u32,
  count: Cell<u32>,
}

impl Sig3 {
  pub fn start() -> Self {
    Self {
      startup: (now_ms() / 1000) as u32,
      count: Cell::new(100),
    }
  }

  /// Sign a call whose query is empty and whose body is `body` (JSON).
  pub fn sign(&self, body: &str) -> String {
    let count = self.count.get();
    self.count.set(count + 1);
    let mut head = Vec::with_capacity(28);
    head.extend(b"TE\x01\x30");
    head.extend(self.startup.to_le_bytes());
    head.extend(count.to_le_bytes());
    head.extend(digest(&hex(&Md5::digest(body.as_bytes()))));
    head.extend(BUILD.to_le_bytes());
    head.extend(TAIL);
    let key = checksum(&head);
    let mut out: Vec<u8> = head
      .iter()
      .enumerate()
      .map(|(i, b)| b ^ key ^ i as u8)
      .collect();
    out.push(key);
    hex(&out)
  }
}

/// Four bytes: SHA-256 compression of the MD5 hex under the first IV, then
/// of that state's big-endian bytes under the second.
fn digest(md5_hex: &str) -> [u8; 4] {
  let first = compress(&blocks(md5_hex.as_bytes()), &iv(&IV_FIRST));
  let mid: Vec<u8> = first.iter().flat_map(|w| w.to_be_bytes()).collect();
  let second = compress(&blocks(&mid), &iv(&IV_SECOND));
  second[0].to_be_bytes()
}

fn iv(words: &[i32; 8]) -> [u32; 8] {
  words.map(|w| w as u32)
}

/// The SDK's padding: `0x80`, big-endian words, a length field of
/// `(len + 63) * 8` in the last block.
fn blocks(msg: &[u8]) -> Vec<[u32; 16]> {
  let mut m = msg.to_vec();
  m.push(0x80);
  let total = ((m.len() as f64 / 4.0 + 2.0) / 16.0).ceil() as usize;
  let mut out = vec![[0u32; 16]; total];
  for (i, byte) in m.iter().enumerate() {
    out[i / 64][(i % 64) / 4] |= u32::from(*byte) << (24 - 8 * (i % 4));
  }
  let bits = ((m.len() + 63) * 8) as u64;
  let last = out.last_mut().expect("at least one block");
  last[14] = (bits >> 32) as u32;
  last[15] = bits as u32;
  out
}

fn compress(blocks: &[[u32; 16]], iv: &[u32; 8]) -> [u32; 8] {
  let mut h = *iv;
  for block in blocks {
    let mut w = [0u32; 64];
    w[..16].copy_from_slice(block);
    for t in 16..64 {
      let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
      let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
      w[t] = w[t - 16]
        .wrapping_add(s0)
        .wrapping_add(w[t - 7])
        .wrapping_add(s1);
    }
    let mut s = h;
    for t in 0..64 {
      let [a, b, c, d, e, f, g, hh] = s;
      let t1 = hh
        .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
        .wrapping_add((e & f) ^ (!e & g))
        .wrapping_add(SHA256_K[t])
        .wrapping_add(w[t]);
      let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
        .wrapping_add((a & b) ^ (a & c) ^ (b & c));
      s = [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g];
    }
    for (x, y) in h.iter_mut().zip(s) {
      *x = x.wrapping_add(y);
    }
  }
  h
}
