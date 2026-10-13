//! What both routes need of files they send.

use std::path::Path;

/// Telegram sends JPEG, PNG and WebP files as photos; everything else
/// (GIFs included) goes as a document.
pub fn is_photo(path: &Path) -> bool {
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .map(str::to_ascii_lowercase)
    .unwrap_or_default();
  matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
}
