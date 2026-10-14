//! Static map images (`/v3/staticmap`) saved to a file.

use std::path::Path;

use atp_core::geo::{image_size, save_map};
use atp_core::{Data, Result};

use crate::api::Api;
use crate::refs;

pub struct MapOptions<'a> {
  pub zoom: u8,
  /// `WIDTH*HEIGHT` in pixels, at most 1024*1024.
  pub size: &'a str,
  /// 2 for high-DPI images.
  pub scale: u8,
  pub traffic: bool,
}

pub async fn staticmap(
  api: &Api,
  location: &str,
  options: &MapOptions<'_>,
  output: &Path,
) -> Result<Data> {
  let location = refs::expect_location(location, api.system())?;
  let (width, height) = image_size(options.size, (1024, 1024))?;
  let params = [
    ("location", location.clone()),
    ("zoom", options.zoom.clamp(1, 17).to_string()),
    ("size", format!("{width}*{height}")),
    (
      "scale",
      if options.scale >= 2 { "2" } else { "1" }.to_owned(),
    ),
    ("markers", format!("mid,,A:{location}")),
    ("traffic", if options.traffic { "1" } else { "" }.to_owned()),
  ];
  let bytes = api.bytes("/v3/staticmap", &params).await?;
  save_map(output, &bytes).await
}
