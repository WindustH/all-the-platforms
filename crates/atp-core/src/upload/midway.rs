//! SHA-1 midway through a file: where the hash stands after the parts sent
//! so far. Some object stores check each part against it (Quark sends it as
//! `X-Oss-Hash-Ctx`), which `sha1::Sha1` does not expose, so the chaining
//! words are kept here with the crate's own block function.

use sha1::digest::generic_array::GenericArray;

use crate::error::{Error, Result};

const BLOCK: usize = 64;
const INITIAL: [u32; 5] = [
  0x6745_2301,
  0xEFCD_AB89,
  0x98BA_DCFE,
  0x1032_5476,
  0xC3D2_E1F0,
];

/// The five chaining words of SHA-1 and how many bytes went in. Only whole
/// 64-byte blocks go in, so no partial block is ever pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sha1Midway {
  pub words: [u32; 5],
  pub bytes: u64,
}

impl Default for Sha1Midway {
  fn default() -> Self {
    Self {
      words: INITIAL,
      bytes: 0,
    }
  }
}

impl Sha1Midway {
  /// Hash `data`, a whole number of 64-byte blocks (every part but the
  /// last of an upload whose part size is a multiple of 64).
  pub fn update(&mut self, data: &[u8]) -> Result<()> {
    let (blocks, rest) = data.as_chunks::<BLOCK>();
    if !rest.is_empty() {
      return Err(Error::internal(format!(
        "a part of {} bytes is not a whole number of SHA-1 blocks",
        data.len()
      )));
    }
    for block in blocks {
      let block = GenericArray::from_slice(block);
      sha1::compress(&mut self.words, std::slice::from_ref(block));
    }
    self.bytes += data.len() as u64;
    Ok(())
  }

  /// Bits hashed so far: the low and the high 32 bits (OpenSSL's `Nl`, `Nh`).
  pub fn bits(&self) -> (u32, u32) {
    let bits = self.bytes.wrapping_mul(8);
    (bits as u32, (bits >> 32) as u32)
  }
}
