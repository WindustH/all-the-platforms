//! Files uploaded as KOOK assets.

use std::path::Path;

use atp_core::file::Upload;
use atp_core::http::Part;
use atp_core::{Error, MediaKind, Result, Value, ValueExt};

use crate::api::Api;

/// Upload a file as an asset; the upload's reply (with its `url`).
pub async fn asset(api: &Api, path: &Path) -> Result<Value> {
  let file = Upload::read(path).await?;
  let part = Part::file("file", file.data, &file.name, file.mime);
  api.upload(vec![part]).await
}

/// Upload a file for a message: its message type (2 image, 3 video, 8
/// audio, 4 file) and asset URL.
pub async fn upload(api: &Api, path: &Path) -> Result<(u64, String)> {
  let file = Upload::read(path).await?;
  let kind = match file.kind() {
    MediaKind::Image | MediaKind::Gif => 2,
    MediaKind::Video => 3,
    MediaKind::Audio => 8,
    MediaKind::File => 4,
  };
  let part = Part::file("file", file.data, &file.name, file.mime);
  let v = api.upload(vec![part]).await?;
  let url = v
    .str("url")
    .ok_or_else(|| Error::upstream("KOOK returned no asset URL"))?;
  Ok((kind, url))
}
