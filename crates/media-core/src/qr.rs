//! Show a login QR code in the terminal and save it as an SVG next to the cache.
//! A QR code handed out as an image is decoded first and shown the same way.

use std::path::{Path, PathBuf};

use qrcode::QrCode;
use qrcode::render::{svg, unicode};

use crate::error::{Error, Result};

/// Show a QR code the platform sent as an image. When it cannot be decoded
/// the image itself is saved (`login-qr.<ext>`) for the user to open.
pub fn show_image(bytes: &[u8], dir: &Path) -> Result<PathBuf> {
  if let Some(data) = decode(bytes) {
    return show(&data, dir);
  }
  let ext = crate::file::sniff(bytes).unwrap_or("png");
  std::fs::create_dir_all(dir)?;
  let path = dir.join(format!("login-qr.{ext}"));
  std::fs::write(&path, bytes)?;
  eprintln!("(the QR code could not be shown here; open the image below)");
  Ok(path)
}

fn decode(bytes: &[u8]) -> Option<String> {
  let gray = image::load_from_memory(bytes).ok()?.to_luma8();
  let (w, h) = gray.dimensions();
  let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w as usize, h as usize, |x, y| {
    gray.get_pixel(x as u32, y as u32)[0]
  });
  let grid = prepared.detect_grids().into_iter().next()?;
  grid.decode().ok().map(|(_, content)| content)
}

/// Print `data` as a QR code on stderr and write `login-qr.svg` into `dir`.
pub fn show(data: &str, dir: &Path) -> Result<PathBuf> {
  let code = QrCode::new(data.as_bytes()).map_err(|e| Error::internal(format!("qr code: {e}")))?;
  let text = code
    .render::<unicode::Dense1x2>()
    .dark_color(unicode::Dense1x2::Light)
    .light_color(unicode::Dense1x2::Dark)
    .quiet_zone(true)
    .build();
  eprintln!("{text}");
  std::fs::create_dir_all(dir)?;
  let path = dir.join("login-qr.svg");
  let image = code.render::<svg::Color>().min_dimensions(256, 256).build();
  std::fs::write(&path, image)?;
  Ok(path)
}
