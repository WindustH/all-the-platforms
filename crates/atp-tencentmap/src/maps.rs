//! Static maps (`/ws/staticmap/v2/`): a map image centred on a point,
//! saved to a file.

use std::path::Path;

use atp_core::geo::{image_size, save_map};
use atp_core::{Data, Result};

use crate::api::Api;
use crate::refs;

pub struct MapOptions<'a> {
  pub zoom: u8,
  /// `WIDTH*HEIGHT` in pixels, at most 1680*1200.
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
  let center = refs::expect_point(location, api.system())?.lat_lng();
  let (width, height) = image_size(options.size, (1680, 1200))?;
  let scale = options.scale.clamp(1, 2);
  let max_zoom = if scale == 2 { 17 } else { 18 };
  let params = [
    ("center", center.clone()),
    ("zoom", options.zoom.clamp(4, max_zoom).to_string()),
    ("size", format!("{}*{}", width.max(50), height.max(50))),
    ("scale", scale.to_string()),
    ("markers", format!("color:blue|{center}")),
  ];
  let bytes = api.image("/ws/staticmap/v2/", &params).await?;
  save_map(output, &bytes).await
}
