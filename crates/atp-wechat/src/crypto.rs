//! Media encryption on the c2c CDN: AES-128-ECB with PKCS#7 padding and a
//! random key per file. Keys travel as base64 of either the 16 raw bytes or
//! their 32-character hex form (both occur).

use aes::Aes128;
use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use atp_core::{Error, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

const BLOCK: usize = 16;

pub fn encrypt(key: &[u8; 16], data: &[u8]) -> Vec<u8> {
  let cipher = Aes128::new(GenericArray::from_slice(key));
  let pad = BLOCK - data.len() % BLOCK;
  let mut out = Vec::with_capacity(data.len() + pad);
  out.extend_from_slice(data);
  out.extend(std::iter::repeat_n(pad as u8, pad));
  for block in out.as_chunks_mut::<BLOCK>().0 {
    cipher.encrypt_block(GenericArray::from_mut_slice(block));
  }
  out
}

pub fn decrypt(key: &[u8; 16], data: &[u8]) -> Result<Vec<u8>> {
  if data.is_empty() || !data.len().is_multiple_of(BLOCK) {
    return Err(Error::upstream("encrypted media has a bad length"));
  }
  let cipher = Aes128::new(GenericArray::from_slice(key));
  let mut out = data.to_vec();
  for block in out.as_chunks_mut::<BLOCK>().0 {
    cipher.decrypt_block(GenericArray::from_mut_slice(block));
  }
  let pad = *out.last().unwrap_or(&0) as usize;
  if (1..=BLOCK).contains(&pad) && out[out.len() - pad..].iter().all(|&b| b as usize == pad) {
    out.truncate(out.len() - pad);
  }
  Ok(out)
}

/// Size after encryption (padding always adds 1..=16 bytes).
pub fn padded_len(len: usize) -> usize {
  len + (BLOCK - len % BLOCK)
}

pub fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
  if !s.len().is_multiple_of(2) {
    return None;
  }
  (0..s.len())
    .step_by(2)
    .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
    .collect()
}

/// A key given as 32 hex characters (`image_item.aeskey`).
pub fn key_from_hex(s: &str) -> Option<[u8; 16]> {
  unhex(s.trim())?.try_into().ok()
}

/// A key given as base64 of 16 raw bytes or of their hex form (`media.aes_key`).
pub fn key_from_base64(s: &str) -> Option<[u8; 16]> {
  let s = s.trim();
  let padded = format!("{s}{}", "=".repeat((4 - s.len() % 4) % 4));
  let raw = STANDARD.decode(padded).ok()?;
  match raw.len() {
    16 => raw.try_into().ok(),
    32 => key_from_hex(std::str::from_utf8(&raw).ok()?),
    _ => None,
  }
}

/// How an uploaded file's key is sent back: base64 of its hex form.
pub fn key_for_message(key: &[u8; 16]) -> String {
  STANDARD.encode(hex(key))
}
