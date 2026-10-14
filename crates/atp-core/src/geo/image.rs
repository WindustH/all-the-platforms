//! Static map images: their `--size`, and saving them.

use std::path::Path;

use crate::error::{Error, Result};
use crate::model::{Data, Downloaded, MediaKind};

/// `--size`: `WIDTH*HEIGHT` or `WIDTHxHEIGHT`, each side from 1 up to
/// `max`.
pub fn image_size(arg: &str, max: (u32, u32)) -> Result<(u32, u32)> {
  let bad = || {
    Error::input(format!(
      "--size `{arg}` is not WIDTH*HEIGHT (at most {}*{})",
      max.0, max.1
    ))
  };
  let (w, h) = arg.split_once(['*', 'x', 'X']).ok_or_else(bad)?;
  let side = |s: &str, max: u32| {
    s.trim()
      .parse::<u32>()
      .ok()
      .filter(|n| (1..=max).contains(n))
  };
  match (side(w, max.0), side(h, max.1)) {
    (Some(w), Some(h)) => Ok((w, h)),
    _ => Err(bad()),
  }
}

/// Write a map image to `output`.
pub async fn save_map(output: &Path, bytes: &[u8]) -> Result<Data> {
  tokio::fs::write(output, bytes)
    .await
    .map_err(|e| Error::input(format!("cannot write {}: {e}", output.display())))?;
  Ok(Data::Downloads(vec![Downloaded {
    kind: MediaKind::Image,
    path: output.display().to_string(),
    bytes: bytes.len() as u64,
  }]))
}
