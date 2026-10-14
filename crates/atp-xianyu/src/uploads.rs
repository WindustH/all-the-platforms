//! Images for chat messages, uploaded to Xianyu's file service.

use std::path::Path;

use atp_core::file::Upload;
use atp_core::http::Part;
use atp_core::{Error, Result, ValueExt};

use crate::api::{Api, WWW};

const UPLOAD: &str = "https://stream-upload.goofish.com/api/upload.api";

pub struct Uploaded {
  pub url: String,
  pub width: u64,
  pub height: u64,
}

pub async fn image(api: &Api, path: &Path) -> Result<Uploaded> {
  let file = Upload::read(path).await?;
  if !file.is_image() {
    return Err(Error::input(format!(
      "Xianyu chats take images only: {}",
      path.display()
    )));
  }
  let (width, height) = file.size().unwrap_or_default();
  let part = Part::file("file", file.data, &file.name, file.mime);
  let v = api
    .ctx
    .http
    .post(UPLOAD)
    .queries([
      ("floderId", "0"),
      ("appkey", "xy_chat"),
      ("_input_charset", "utf-8"),
    ])
    .header("origin", WWW)
    .header("referer", format!("{WWW}/"))
    .multipart(vec![part])
    .send()
    .await?
    .value()?;
  let url = v.str("object.url").ok_or_else(|| {
    let reason = v.first_str(&["message", "errorMessage", "errorCode"]);
    Error::upstream(format!(
      "Xianyu refused the image: {}",
      reason.unwrap_or_default()
    ))
  })?;
  // `pix` is `<width>x<height>` as the service measured it.
  let pix = v.str("object.pix").unwrap_or_default();
  let (w, h) = pix.split_once('x').unwrap_or_default();
  Ok(Uploaded {
    url,
    width: w.parse().unwrap_or(width as u64),
    height: h.parse().unwrap_or(height as u64),
  })
}
