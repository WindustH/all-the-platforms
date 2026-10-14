//! The ciphers under the request encodings: AES-128 in CBC and ECB mode
//! with PKCS#7 padding, and RSA without padding over a 1024-bit key.

use aes::Aes128;
use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockEncrypt, KeyInit};
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockEncryptMut, KeyIvInit};
use num_bigint::BigUint;

const BLOCK: usize = 16;

/// AES-128-CBC with PKCS#7 padding.
pub fn cbc(key: &[u8; 16], iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
  let mut buf = data.to_vec();
  buf.resize(data.len() + BLOCK, 0);
  let len = cbc::Encryptor::<Aes128>::new(key.into(), iv.into())
    .encrypt_padded_mut::<Pkcs7>(&mut buf, data.len())
    .expect("room for the padding")
    .len();
  buf.truncate(len);
  buf
}

/// AES-128-ECB with PKCS#7 padding.
pub fn ecb(key: &[u8; 16], data: &[u8]) -> Vec<u8> {
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

/// `data ^ exponent mod modulus` as lower-case hex, zero-padded to the
/// modulus' width (the message is taken as a big-endian number).
pub fn rsa(data: &[u8], exponent: u32, modulus_hex: &str) -> String {
  let modulus = BigUint::parse_bytes(modulus_hex.as_bytes(), 16).expect("a hex modulus");
  let sealed = BigUint::from_bytes_be(data).modpow(&BigUint::from(exponent), &modulus);
  let width = modulus_hex.trim_start_matches('0').len().div_ceil(2) * 2;
  format!("{:0>width$}", sealed.to_str_radix(16))
}

pub fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}
