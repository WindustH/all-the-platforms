//! File uploads: prepare, then the file as one chunk; the server path comes
//! back for a `vocechat/file` message.

use std::path::Path;

use social_core::file::Upload;
use social_core::http::{Method, Part};
use social_core::{Error, Result, ValueExt, json};

use crate::api::{Api, Body};

pub async fn upload(api: &Api, path: &Path) -> Result<String> {
  let file = Upload::read(path).await?;
  let base = if api.is_bot() {
    "/bot/file"
  } else {
    "/resource/file"
  };
  let prepared = api
    .call(
      Method::POST,
      &format!("{base}/prepare"),
      Body::Json(json!({"content_type": file.mime, "filename": file.name})),
    )
    .await?;
  let id = prepared
    .as_str()
    .map(str::to_owned)
    .ok_or_else(|| Error::upstream("the upload was not prepared (no file id)"))?;
  let v = api
    .call(
      Method::POST,
      &format!("{base}/upload"),
      Body::Multipart(vec![
        Part::text("file_id", id),
        Part::file("chunk_data", file.data, &file.name, file.mime),
        Part::text("chunk_is_last", "true"),
      ]),
    )
    .await?;
  v.str("path")
    .ok_or_else(|| Error::upstream("the upload returned no file path"))
}
