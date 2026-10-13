//! Files to send: read with social-core's `Upload`, with the MIME type
//! WhatsApp expects.

use std::path::Path;

use social_core::Result;
use social_core::file::{Image, Upload};

/// A file to send; Opus audio is marked as such, as WhatsApp's clients do.
pub async fn read(path: &Path) -> Result<Upload> {
  let mut file = Upload::read(path).await?;
  if file.mime == "audio/ogg" {
    file.mime = "audio/ogg; codecs=opus";
  }
  Ok(file)
}

/// Width and height of an image to send.
pub fn image_size(file: &Upload) -> (u32, u32) {
  let image = Image {
    data: file.data.clone(),
    mime: file.mime,
    name: file.name.clone(),
  };
  let (w, h) = image.size();
  (w as u32, h as u32)
}
