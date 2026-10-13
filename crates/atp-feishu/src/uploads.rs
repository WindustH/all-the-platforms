//! Files for messages: images as `image`, opus audio as `audio`,
//! everything else as `file`.

use std::path::Path;

use atp_core::file::{Upload, sniff};
use atp_core::http::Part;
use atp_core::{Error, Result, Value, ValueExt, json};

use crate::api::Api;

/// The message type and content that send the file at `path`.
pub async fn upload(api: &Api, path: &Path) -> Result<(&'static str, Value)> {
  let file = Upload::read(path).await?;
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or_default()
    .to_ascii_lowercase();
  let image = matches!(sniff(&file.data), Some("jpg" | "png" | "gif" | "webp"))
    || matches!(ext.as_str(), "bmp" | "ico" | "tiff" | "heic");
  if image {
    image_message(api, file).await
  } else {
    file_message(api, file, &ext).await
  }
}

async fn image_message(api: &Api, file: Upload) -> Result<(&'static str, Value)> {
  let parts = vec![
    Part::text("image_type", "message"),
    Part::file("image", file.data, &file.name, "application/octet-stream"),
  ];
  let v = api.upload("/im/v1/images", parts).await?;
  let key = v
    .str("data.image_key")
    .ok_or_else(|| Error::upstream("image upload returned no image_key"))?;
  Ok(("image", json!({ "image_key": key })))
}

async fn file_message(api: &Api, file: Upload, ext: &str) -> Result<(&'static str, Value)> {
  let file_type = match ext {
    "opus" => "opus",
    "mp4" => "mp4",
    "pdf" => "pdf",
    "doc" | "docx" => "doc",
    "xls" | "xlsx" => "xls",
    "ppt" | "pptx" => "ppt",
    _ => "stream",
  };
  let parts = vec![
    Part::text("file_type", file_type),
    Part::text("file_name", file.name.clone()),
    Part::file("file", file.data, &file.name, "application/octet-stream"),
  ];
  let v = api.upload("/im/v1/files", parts).await?;
  let key = v
    .str("data.file_key")
    .ok_or_else(|| Error::upstream("file upload returned no file_key"))?;
  let msg_type = if file_type == "opus" { "audio" } else { "file" };
  Ok((msg_type, json!({ "file_key": key })))
}
