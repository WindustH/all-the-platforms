//! Static maps v2 (`/staticimage/v2`): a map image centred on a point,
//! saved to a file.

use std::path::Path;

use atp_core::geo::{image_size, save_map};
use atp_core::{Data, Result};

use crate::api::Api;
use crate::refs;

pub struct MapOptions<'a> {
  pub zoom: u8,
  /// `WIDTH*HEIGHT` in pixels, at most 1024*1024 (512*512 at scale 2).
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
  let wire = api.wire();
  let center = refs::expect_point(location, api.system(), wire)?.lng_lat();
  let max = if options.scale >= 2 { 512 } else { 1024 };
  let (width, height) = image_size(options.size, (max, max))?;
  let params = [
    ("center", center.clone()),
    ("width", width.to_string()),
    ("height", height.to_string()),
    ("zoom", options.zoom.clamp(3, 18).to_string()),
    (
      "scaler",
      if options.scale >= 2 { "2" } else { "" }.to_owned(),
    ),
    ("coordtype", refs::ll(wire).to_owned()),
    ("markers", center),
    ("copyright", "1".to_owned()),
  ];
  let bytes = api.image("/staticimage/v2", &params).await?;
  save_map(output, &bytes).await
}
