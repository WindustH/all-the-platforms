//! Images uploaded for notes, through the creator center's permits.

use std::path::Path;

use atp_core::Result;
use atp_core::file::Image;

use crate::api::Api;

/// Upload one image; the file id notes refer to it by.
pub async fn image(api: &Api, path: &Path) -> Result<String> {
  let params = [
    ("biz_name", "spectrum"),
    ("scene", "image"),
    ("file_count", "1"),
    ("version", "1"),
    ("source", "web"),
  ];
  let permits = api
    .creator_get("/api/media/v1/upload/web/permit", &params)
    .await?;
  let image = Image::read(path).await?;
  api.upload(&permits, image.data, image.mime).await
}
