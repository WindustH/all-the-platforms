//! What needs ffmpeg: merging tracks, extracting audio, WAV segments.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::error::{Error, Result};

/// `input` -> `dir/seg_000.wav ...` (16 kHz mono PCM).
pub(super) async fn split(input: &Path, dir: &Path, secs: u32) -> Result<Vec<PathBuf>> {
  require_ffmpeg().await?;
  tokio::fs::create_dir_all(dir).await?;
  let pattern = dir.join("seg_%03d.wav");
  let secs = secs.max(1).to_string();
  ffmpeg(&[
    "-i".as_ref(),
    input.as_os_str(),
    "-vn".as_ref(),
    "-ac".as_ref(),
    "1".as_ref(),
    "-ar".as_ref(),
    "16000".as_ref(),
    "-f".as_ref(),
    "segment".as_ref(),
    "-segment_time".as_ref(),
    secs.as_ref(),
    pattern.as_os_str(),
  ])
  .await?;
  let mut segments = Vec::new();
  let mut entries = tokio::fs::read_dir(dir).await?;
  while let Some(e) = entries.next_entry().await? {
    if e.file_name().to_string_lossy().starts_with("seg_") {
      segments.push(e.path());
    }
  }
  segments.sort();
  Ok(segments)
}

pub(super) async fn has_ffmpeg() -> bool {
  tokio::process::Command::new("ffmpeg")
    .arg("-version")
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .await
    .is_ok_and(|s| s.success())
}

pub(super) async fn require_ffmpeg() -> Result<()> {
  if has_ffmpeg().await {
    Ok(())
  } else {
    Err(
      Error::internal("ffmpeg is required for this operation")
        .with_hint("install ffmpeg and make sure it is on PATH"),
    )
  }
}

pub(super) async fn ffmpeg(args: &[&std::ffi::OsStr]) -> Result<()> {
  let output = tokio::process::Command::new("ffmpeg")
    .args(["-hide_banner", "-loglevel", "error", "-y"])
    .args(args)
    .stdin(Stdio::null())
    .output()
    .await?;
  if output.status.success() {
    Ok(())
  } else {
    Err(Error::internal(format!(
      "ffmpeg failed: {}",
      String::from_utf8_lossy(&output.stderr).trim()
    )))
  }
}
