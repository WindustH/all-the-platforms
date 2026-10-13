//! Local files as message segments, sent inline (`base64://`) so the
//! implementation need not see the path.

use std::path::Path;

use atp_core::file::Upload;
use atp_core::{MediaKind, Result, Value, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// The segment type of a file: `image`, `video`, `record` (voice) or `file`.
fn segment_type(file: &Upload) -> &'static str {
  let ext = file
    .name
    .rsplit_once('.')
    .map(|(_, e)| e.to_ascii_lowercase())
    .unwrap_or_default();
  match (file.kind(), ext.as_str()) {
    (MediaKind::Image | MediaKind::Gif, _) => "image",
    (MediaKind::Video, _) | (_, "avi") => "video",
    (MediaKind::Audio, _) | (_, "silk") => "record",
    _ => "file",
  }
}

pub async fn segment(path: &Path) -> Result<Value> {
  let file = Upload::read(path).await?;
  let kind = segment_type(&file);
  let data = format!("base64://{}", STANDARD.encode(&file.data));
  Ok(match kind {
    "file" => json!({"type": "file", "data": {"file": data, "name": file.name}}),
    kind => json!({"type": kind, "data": {"file": data}}),
  })
}
