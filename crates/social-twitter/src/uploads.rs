//! Image upload for new tweets: the chunked `media/upload` API
//! (INIT → APPEND → FINALIZE) with one base64 segment, as the reference does.

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use social_core::file::Image;
use social_core::{Error, Result, ValueExt};

use crate::api::Api;

const UPLOAD: &str = "https://upload.twitter.com/i/media/upload.json";
const MAX_BYTES: u64 = 5 * 1024 * 1024;
const MAX_IMAGES: usize = 4;

/// Upload the images (at most four) of a new tweet.
/// Upload the images of a tweet (at most four); their media ids.
pub async fn images(api: &Api, images: &[PathBuf]) -> Result<Vec<String>> {
  if images.len() > MAX_IMAGES {
    return Err(Error::input(format!(
      "a tweet takes at most {MAX_IMAGES} images"
    )));
  }
  api.require_login()?;
  let mut ids = Vec::new();
  for path in images {
    ids.push(image(api, path).await?);
  }
  Ok(ids)
}

/// Upload one image for a tweet; returns its media id.
pub async fn image(api: &Api, path: &Path) -> Result<String> {
  upload(api, path, false).await
}

/// Upload one image for a direct message (`dm_image` / `dm_gif`).
pub async fn dm_image(api: &Api, path: &Path) -> Result<String> {
  upload(api, path, true).await
}

/// An image X takes: jpeg, png, gif or webp, at most 5 MB.
async fn read(path: &Path) -> Result<Image> {
  let image = Image::read(path).await?;
  if !matches!(
    image.mime,
    "image/jpeg" | "image/png" | "image/gif" | "image/webp"
  ) {
    return Err(Error::input(format!(
      "unsupported image {} (jpeg, png, gif or webp)",
      path.display()
    )));
  }
  let size = image.data.len() as u64;
  if size > MAX_BYTES {
    return Err(Error::input(format!(
      "{} is {:.1} MB; images may be at most 5 MB",
      path.display(),
      size as f64 / 1048576.0
    )));
  }
  Ok(image)
}

async fn upload(api: &Api, path: &Path, dm: bool) -> Result<String> {
  let Image {
    data,
    mime: media_type,
    ..
  } = read(path).await?;
  let size = data.len().to_string();
  let category = if media_type == "image/gif" {
    "dm_gif"
  } else {
    "dm_image"
  };
  let mut init_form = vec![
    ("command", "INIT"),
    ("total_bytes", size.as_str()),
    ("media_type", media_type),
  ];
  if dm {
    init_form.push(("media_category", category));
  }
  let init = api.post_form(UPLOAD, &init_form).await?;
  let id = init
    .str("media_id_string")
    .ok_or_else(|| Error::upstream("media upload INIT returned no media id"))?;
  let encoded = STANDARD.encode(&data);
  api
    .post_form(
      UPLOAD,
      &[
        ("command", "APPEND"),
        ("media_id", &id),
        ("segment_index", "0"),
        ("media_data", &encoded),
      ],
    )
    .await?;
  api
    .post_form(UPLOAD, &[("command", "FINALIZE"), ("media_id", &id)])
    .await?;
  Ok(id)
}
