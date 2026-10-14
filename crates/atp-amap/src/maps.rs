//! Static map images (`/v3/staticmap`) saved to a file.

use std::path::Path;

use atp_core::{Data, Downloaded, Error, MediaKind, Result};

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
  let location = refs::expect_location(location)?;
  let params = [
    ("location", location.clone()),
    ("zoom", options.zoom.clamp(1, 17).to_string()),
    ("size", size(options.size)?),
    (
      "scale",
      if options.scale >= 2 { "2" } else { "1" }.to_owned(),
    ),
    ("markers", format!("mid,,A:{location}")),
    ("traffic", if options.traffic { "1" } else { "" }.to_owned()),
  ];
  let bytes = api.bytes("/v3/staticmap", &params).await?;
  tokio::fs::write(output, &bytes)
    .await
    .map_err(|e| Error::input(format!("cannot write {}: {e}", output.display())))?;
  Ok(Data::Downloads(vec![Downloaded {
    kind: MediaKind::Image,
    path: output.display().to_string(),
    bytes: bytes.len() as u64,
  }]))
}

/// `WxH` or `W*H`, each side 1–1024.
fn size(arg: &str) -> Result<String> {
  let bad = || {
    Error::input(format!(
      "--size `{arg}` is not WIDTH*HEIGHT (at most 1024*1024)"
    ))
  };
  let (w, h) = arg.split_once(['*', 'x', 'X']).ok_or_else(bad)?;
  let side = |s: &str| {
    s.trim()
      .parse::<u32>()
      .ok()
      .filter(|n| (1..=1024).contains(n))
  };
  match (side(w), side(h)) {
    (Some(w), Some(h)) => Ok(format!("{w}*{h}")),
    _ => Err(bad()),
  }
}
