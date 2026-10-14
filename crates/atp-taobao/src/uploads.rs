//! Images for chat messages, uploaded to Taobao's file service.

use std::path::Path;

use atp_core::file::Upload;
use atp_core::http::{Part, USER_AGENT};
use atp_core::{Error, Result, Value, ValueExt};

use crate::api::Api;

const UPLOAD: &str = "https://stream-upload.taobao.com/api/upload.api";
const ORIGIN: &str = "https://market.m.taobao.com";

/// What the service answers (`object`: `fileId`, `url`, `size`, `pix`),
/// with the size measured here when it does not say.
pub struct Uploaded {
  pub object: Value,
  pub width: u64,
  pub height: u64,
}

pub async fn image(api: &Api, path: &Path) -> Result<Uploaded> {
  let file = Upload::read(path).await?;
  if !file.is_image() {
    return Err(Error::input(format!(
      "Taobao chats take images only: {}",
      path.display()
    )));
  }
  let (width, height) = file.size().unwrap_or_default();
  let parts = vec![
    Part::text("name", file.name.clone()),
    Part::text("ua", USER_AGENT),
    Part::file("file", file.data, &file.name, file.mime),
  ];
  let v = api
    .ctx
    .http
    .post(UPLOAD)
    .queries([
      ("appkey", "ampmedia"),
      ("folderId", "0"),
      ("_input_charset", "utf-8"),
      ("useGtrSessionFilter", "false"),
    ])
    .header("origin", ORIGIN)
    .header("referer", format!("{ORIGIN}/"))
    .multipart(parts)
    .send()
    .await?
    .value()?;
  if v.bool("success") != Some(true) || v.str("object.url").is_none() {
    let reason = v.first_str(&["message", "errorCode"]).unwrap_or_default();
    if reason.contains("NOT_LOGIN") || reason.contains("SESSION") {
      return Err(Error::auth(format!("Taobao refused the upload: {reason}")));
    }
    return Err(Error::upstream(format!(
      "Taobao refused the image: {reason}"
    )));
  }
  let pix = v.str("object.pix").unwrap_or_default();
  let (w, h) = pix.split_once('x').unwrap_or_default();
  Ok(Uploaded {
    width: w.parse().unwrap_or(width as u64),
    height: h.parse().unwrap_or(height as u64),
    object: v.at("object").clone(),
  })
}
