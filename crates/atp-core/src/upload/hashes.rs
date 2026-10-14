//! Digests of a file from one read of it: what drives compare before they
//! take a file, and how they find one they already hold (instant upload).

use std::path::Path;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use tokio::io::AsyncReadExt;

use crate::error::Result;
use crate::output::{progress, set_total};

/// Block size of [`Hashes::blocks`]: 4 MB, Baidu's upload block.
pub const BLOCK: u64 = 4 << 20;
/// Length of the head [`Hashes::head_md5`] covers: 256 KB, Baidu's `slice-md5`.
pub const HEAD: u64 = 256 << 10;

/// Digests of a whole file, lower-case hex.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hashes {
  pub md5: String,
  pub sha1: String,
  /// md5 of the first 256 KB (all of a smaller file).
  pub head_md5: String,
  /// md5 of each 4 MB block, the last one shorter; one entry for an empty file.
  pub blocks: Vec<String>,
}

impl Hashes {
  /// Read `path` once; `size` sizes the progress bar.
  pub async fn of(path: &Path, size: u64) -> Result<Self> {
    let mut file = tokio::fs::File::open(path).await?;
    let label = path
      .file_name()
      .map(|n| format!("hashing {}", n.to_string_lossy()))
      .unwrap_or_default();
    let bar = progress(&label);
    set_total(&bar, size);
    let mut pass = Pass::default();
    let mut buf = vec![0u8; 1 << 20];
    loop {
      let n = file.read(&mut buf).await?;
      if n == 0 {
        break;
      }
      pass.update(&buf[..n]);
      bar.inc(n as u64);
    }
    bar.finish_and_clear();
    Ok(pass.finish())
  }
}

/// The digests while the file streams through.
#[derive(Default)]
struct Pass {
  md5: Md5,
  sha1: Sha1,
  head: Md5,
  block: Md5,
  /// Bytes in the current block.
  block_len: u64,
  /// Bytes seen so far.
  seen: u64,
  blocks: Vec<String>,
}

impl Pass {
  fn update(&mut self, data: &[u8]) {
    self.md5.update(data);
    self.sha1.update(data);
    if self.seen < HEAD {
      let take = (HEAD - self.seen).min(data.len() as u64) as usize;
      self.head.update(&data[..take]);
    }
    self.seen += data.len() as u64;
    let mut rest = data;
    while !rest.is_empty() {
      let take = (BLOCK - self.block_len).min(rest.len() as u64) as usize;
      self.block.update(&rest[..take]);
      self.block_len += take as u64;
      rest = &rest[take..];
      if self.block_len == BLOCK {
        self
          .blocks
          .push(format!("{:x}", self.block.finalize_reset()));
        self.block_len = 0;
      }
    }
  }

  fn finish(mut self) -> Hashes {
    if self.block_len > 0 || self.blocks.is_empty() {
      self.blocks.push(format!("{:x}", self.block.finalize()));
    }
    Hashes {
      md5: format!("{:x}", self.md5.finalize()),
      sha1: format!("{:x}", self.sha1.finalize()),
      head_md5: format!("{:x}", self.head.finalize()),
      blocks: self.blocks,
    }
  }
}
