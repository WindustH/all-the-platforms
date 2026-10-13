//! Files for robot messages: images as `sampleImageMsg`, others as
//! `sampleFile`, both uploaded through `media/upload` first.

use std::path::Path;

use atp_core::file::{Upload, sniff};
use atp_core::http::Part;
use atp_core::{Error, Result, Value, ValueExt, json};

use crate::api::{Api, OapiBody};

/// The message key and parameters that send the file at `path`.
pub async fn upload(api: &Api, path: &Path) -> Result<(&'static str, Value)> {
  let file = Upload::read(path).await?;
  let image = matches!(sniff(&file.data), Some("jpg" | "png" | "gif"));
  let kind = if image { "image" } else { "file" };
  let name = file.name.clone();
  let part = Part::file("media", file.data, &name, "application/octet-stream");
  let v = api
    .oapi(
      "/media/upload",
      &[("type", kind.to_owned())],
      OapiBody::Multipart(vec![part]),
    )
    .await?;
  let media_id = v
    .str("media_id")
    .ok_or_else(|| Error::upstream("media upload returned no media_id"))?;
  if image {
    return Ok(("sampleImageMsg", json!({ "photoURL": media_id })));
  }
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or("bin")
    .to_owned();
  Ok((
    "sampleFile",
    json!({ "mediaId": media_id, "fileName": name, "fileType": ext }),
  ))
}
