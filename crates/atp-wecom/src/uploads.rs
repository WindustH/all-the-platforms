//! Files for messages: images as `image`, `.amr` as `voice`, `.mp4` as
//! `video`, everything else as `file`. Webhooks take images inline (base64
//! + md5) and other files through their own upload.

use std::path::Path;

use atp_core::file::{Upload, sniff};
use atp_core::http::Part;
use atp_core::{Error, Result, Value, ValueExt, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use md5::{Digest, Md5};

use crate::api::{Api, Body};
use crate::refs::Target;

/// The message type and body that send the file at `path` to `target`.
pub async fn upload(api: &Api, target: &Target, path: &Path) -> Result<(&'static str, Value)> {
  let file = Upload::read(path).await?;
  let image = matches!(sniff(&file.data), Some("jpg" | "png"));
  match target {
    Target::Webhook(_) if image => Ok(("image", inline_image(&file.data))),
    Target::Webhook(key) => webhook_file(api, key, file).await,
    _ => app_media(api, media_type(image, path), file).await,
  }
}

fn media_type(image: bool, path: &Path) -> &'static str {
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or_default()
    .to_ascii_lowercase();
  match ext.as_str() {
    _ if image => "image",
    "amr" => "voice",
    "mp4" => "video",
    _ => "file",
  }
}

/// A robot webhook image: base64 with its md5.
fn inline_image(data: &[u8]) -> Value {
  let md5: String = Md5::digest(data)
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect();
  json!({ "base64": STANDARD.encode(data), "md5": md5 })
}

fn media_part(file: Upload) -> Body {
  Body::Multipart(vec![Part::file(
    "media",
    file.data,
    &file.name,
    "application/octet-stream",
  )])
}

async fn webhook_file(api: &Api, key: &str, file: Upload) -> Result<(&'static str, Value)> {
  let query = [("type", "file".to_owned())];
  let v = api
    .webhook("/webhook/upload_media", key, &query, media_part(file))
    .await?;
  Ok(("file", json!({ "media_id": v.str("media_id") })))
}

async fn app_media(api: &Api, kind: &'static str, file: Upload) -> Result<(&'static str, Value)> {
  let query = [("type", kind.to_owned())];
  let v = api.call("/media/upload", &query, media_part(file)).await?;
  let media_id = v
    .str("media_id")
    .ok_or_else(|| Error::upstream("media upload returned no media_id"))?;
  Ok((kind, json!({ "media_id": media_id })))
}
