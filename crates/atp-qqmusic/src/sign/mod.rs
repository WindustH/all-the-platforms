//! The computations QQ Music and QQ login ask for, all pure:
//!
//! - [`zzc`]: the `sign` query parameter of a gateway call (`zzc…`). The
//!   SHA-1 of the exact request body as upper-case hex; eight characters
//!   picked from it at fixed places, then 20 bytes (each byte of the digest
//!   XORed with a fixed table) in base64 without `+`, `/`, `\` and `=`,
//!   then eight more picked characters; all lower case.
//! - [`hash33`]: `h = h * 33 + c`, kept to 31 bits: the `g_tk` of a key
//!   (from 5381) and the `ptqrtoken` of a QR code's `qrsig` (from 0).
//!
//! As in the web client's request layer and in open ports of it (lx-music,
//! RustMusic, netease-qq-music-api), which publish the vectors it was
//! checked against; the gateway answers a wrong `sign` with code 2000.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha1::{Digest, Sha1};

/// Places of the first part; 40 is past the end of the digest and gives
/// nothing, as in the original.
const HEAD: [usize; 8] = [23, 14, 6, 36, 16, 40, 7, 19];
/// Places of the last part.
const TAIL: [usize; 8] = [16, 1, 32, 12, 19, 27, 8, 5];
/// What each byte of the digest is XORed with.
const SCRAMBLE: [u8; 20] = [
  89, 39, 179, 150, 218, 82, 58, 252, 177, 52, 186, 123, 120, 64, 242, 133, 143, 161, 121, 179,
];

/// The `sign` of a gateway request whose body is `text`.
pub fn zzc(text: &str) -> String {
  let digest = Sha1::digest(text.as_bytes());
  let hex: Vec<u8> = digest
    .iter()
    .flat_map(|b| format!("{b:02X}").into_bytes())
    .collect();
  let pick = |places: &[usize]| -> String {
    places
      .iter()
      .filter_map(|&i| hex.get(i).map(|&c| c as char))
      .collect()
  };
  let mixed: Vec<u8> = digest
    .iter()
    .zip(SCRAMBLE)
    .map(|(byte, key)| byte ^ key)
    .collect();
  let middle: String = STANDARD
    .encode(mixed)
    .chars()
    .filter(|c| !matches!(c, '+' | '/' | '\\' | '='))
    .collect();
  format!("zzc{}{middle}{}", pick(&HEAD), pick(&TAIL)).to_ascii_lowercase()
}

/// QQ's 33-times hash of `text`, starting from `seed`, in 31 bits.
pub fn hash33(text: &str, seed: u32) -> u32 {
  let mut hash = seed;
  for c in text.chars() {
    hash = hash
      .wrapping_shl(5)
      .wrapping_add(hash)
      .wrapping_add(c as u32);
  }
  hash & 0x7fff_ffff
}
