//! Maps Static API: a map image centred on a point, saved to a file.

use std::path::Path;

use atp_core::geo::{image_size, save_map};
use atp_core::output::note;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::refs;

pub struct MapOptions<'a> {
  pub zoom: u8,
  /// `WIDTH*HEIGHT` in pixels, at most 640*640.
  pub size: &'a str,
  /// 2 for high-DPI images.
  pub scale: u8,
}

pub async fn staticmap(
  api: &Api,
  location: &str,
  options: &MapOptions<'_>,
  output: &Path,
) -> Result<Data> {
  let point = refs::expect_point(location, api.system())?.lat_lng();
  let (width, height) = image_size(options.size, (640, 640))?;
  let params = [
    ("center", point.clone()),
    ("zoom", options.zoom.min(21).to_string()),
    ("size", format!("{width}x{height}")),
    ("scale", options.scale.clamp(1, 2).to_string()),
    ("markers", format!("size:mid|{point}")),
  ];
  let resp = api.image("/staticmap", &params).await?;
  if let Some(warning) = resp.header("x-staticmap-api-warning") {
    note(&format!("Google: {warning}"));
  }
  save_map(output, &resp.body).await
}
