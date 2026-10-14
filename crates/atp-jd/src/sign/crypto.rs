//! The hash primitives of JD's `js_security_v3` (h5st 5.3): CryptoJS whose
//! string inputs are salted, whose hex output starts with seven reversed
//! bytes and whose HMAC scrambles string keys, plus the library's two
//! base64 forms over its own alphabet.

use md5::{Digest, Md5};
use sha2::Sha256;

/// Characters of the salt appended to every hashed string.
const SALT_ALPHABET: &[u8; 64] =
  b"nmlkjihgfedcbaZYXWVUTSRQPONMLKJIHGFEDCBA-_9876543210zyxwvutsrqpo";
const SALT_GROUPS: usize = 9;
/// Appended after the salt before the bytes are hashed.
const TAIL: &str = "=@3k4k";
/// The library's base64 alphabet.
pub const ALPHABET: &[u8; 64] = b"rqponmlkjihgfedcbaZYXWVUTSRQPONMLKJIHGFEDCBA-_9876543210zyxwvuts";
/// HMAC block size of both hashes, in bytes.
const BLOCK: usize = 64;

/// The two hashes the signer and the server's key recipes use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hash {
  Md5,
  Sha256,
}

impl Hash {
  /// The plain digest of bytes (what the library does with byte arrays).
  pub fn digest(self, data: &[u8]) -> Vec<u8> {
    match self {
      Hash::Md5 => Md5::digest(data).to_vec(),
      Hash::Sha256 => Sha256::digest(data).to_vec(),
    }
  }

  /// A string hashed the library's way, as its hex.
  pub fn text(self, text: &str) -> String {
    hex(&self.digest(salted(text).as_bytes()))
  }

  /// HMAC of a string under a string key, as its hex.
  pub fn hmac(self, text: &str, key: &str) -> String {
    let mut key = scramble(key).into_bytes();
    if key.len() > BLOCK {
      key = self.digest(&key);
    }
    key.resize(BLOCK, 0);
    // The HMAC salts the message once more and drops two characters
    // before the hasher salts it again.
    let once = salt(text);
    let message = salted(&once[..once.len() - 2]);
    let mut inner: Vec<u8> = key.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(message.as_bytes());
    let mut outer: Vec<u8> = key.iter().map(|b| b ^ 0x5c).collect();
    outer.extend(self.digest(&inner));
    hex(&self.digest(&outer))
  }
}

/// `text` with one salt character per ninth of it (the last ninth takes
/// the remainder), counted in UTF-16 units like the browser.
fn salt(text: &str) -> String {
  let units: Vec<u16> = text.encode_utf16().collect();
  let part = units.len() / SALT_GROUPS;
  let mut out = String::from(text);
  for group in 0..SALT_GROUPS {
    let size = if group == SALT_GROUPS - 1 {
      part + units.len() % SALT_GROUPS
    } else {
      part
    };
    let sum: u64 = units
      .iter()
      .skip(group * part)
      .take(size)
      .map(|&u| u64::from(u))
      .sum();
    out.push(SALT_ALPHABET[(sum * 3 % 64) as usize] as char);
  }
  out
}

fn salted(text: &str) -> String {
  format!("{}{TAIL}", salt(text))
}

/// The HMAC key form: the first 16 characters reversed and shifted within
/// printable ASCII, the rest as is.
fn scramble(key: &str) -> String {
  let chars: Vec<char> = key.chars().collect();
  let head = chars.len().min(16);
  let shifted = chars[..head].iter().rev().map(|&c| {
    let code = (7 * (c as i64 - 32) + 52).rem_euclid(95) + 32;
    char::from_u32(code as u32).unwrap_or(c)
  });
  shifted.chain(chars[head..].iter().copied()).collect()
}

/// Hex as the library prints a digest: its first seven bytes reversed.
pub fn hex(digest: &[u8]) -> String {
  let mut bytes = digest.to_vec();
  let head = bytes.len().min(7);
  bytes[..head].reverse();
  plain_hex(&bytes)
}

pub fn plain_hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The library's `Base64.encode`: padded to whole triples with the pad
/// length, triples in reverse order, each quad of the output reversed.
pub fn encode(data: &[u8]) -> String {
  let pad = 3 - data.len() % 3;
  let mut bytes = data.to_vec();
  bytes.extend(std::iter::repeat_n(pad as u8, pad));
  let reordered: Vec<u8> = bytes.chunks(3).rev().flatten().copied().collect();
  stringify(&reordered)
    .as_bytes()
    .chunks(4)
    .flat_map(|quad| quad.iter().rev())
    .map(|&b| b as char)
    .collect()
}

/// Plain base64 over the library's alphabet, without padding.
pub fn stringify(data: &[u8]) -> String {
  let mut out = String::new();
  for chunk in data.chunks(3) {
    let mut block = [0u8; 3];
    block[..chunk.len()].copy_from_slice(chunk);
    let value = u32::from(block[0]) << 16 | u32::from(block[1]) << 8 | u32::from(block[2]);
    for i in 0..=chunk.len() {
      out.push(ALPHABET[(value >> (18 - 6 * i) & 63) as usize] as char);
    }
  }
  out
}

/// Plain SHA-256 hex: what pages sign in place of a request body.
pub fn body_hash(body: &str) -> String {
  plain_hex(&Sha256::digest(body.as_bytes()))
}
