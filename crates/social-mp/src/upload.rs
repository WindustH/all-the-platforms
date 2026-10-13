//! Image uploads for drafts.
//!
//! - API: `media/uploadimg` gives a URL for article bodies (JPEG / PNG under
//!   1 MB); `material/add_material?type=image` gives the permanent
//!   `media_id` a cover needs.
//! - Backend: `filetransfer?action=upload_material` with the ticket of the
//!   home page gives a file id and a CDN URL, used for both.

use std::path::Path;

use social_core::file::Image;
use social_core::http::Part;
use social_core::{Error, Result, ValueExt};

use crate::account::Home;
use crate::client::Client;
use crate::web::MP;

/// URL of an image for an article body, through the API.
pub async fn api_body_image(c: &Client, path: &Path) -> Result<String> {
  let image = Image::read(path).await?;
  if !matches!(image.mime, "image/jpeg" | "image/png") || image.data.len() >= 1 << 20 {
    return Err(Error::input(format!(
      "{}: article images through the API must be JPEG or PNG under 1 MB",
      path.display()
    )));
  }
  let part = Part::file("media", image.data, &image.name, image.mime);
  let v = c
    .api_upload("/cgi-bin/media/uploadimg", &[], part, Vec::new())
    .await?;
  v.str("url")
    .ok_or_else(|| Error::upstream("uploadimg returned no url"))
}

/// A permanent material (`image`, `thumb`, `voice`, `video`): its media id
/// and, for images, its URL.
pub async fn api_material(
  c: &Client,
  path: &Path,
  kind: &str,
  video: Option<(&str, &str)>,
) -> Result<(String, Option<String>)> {
  let data = tokio::fs::read(path)
    .await
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  let mime = social_core::file::sniff(&data)
    .map(|ext| match ext {
      "jpg" => "image/jpeg",
      "png" => "image/png",
      "gif" => "image/gif",
      _ => "application/octet-stream",
    })
    .unwrap_or("application/octet-stream");
  let mut fields = Vec::new();
  if let Some((title, intro)) = video {
    let desc = social_core::json!({ "title": title, "introduction": intro });
    fields.push(Part::text("description", desc.to_string()));
  }
  let v = c
    .api_upload(
      "/cgi-bin/material/add_material",
      &[("type", kind.to_owned())],
      Part::file("media", data, &name, mime),
      fields,
    )
    .await?;
  let id = v
    .str("media_id")
    .ok_or_else(|| Error::upstream("add_material returned no media_id"))?;
  Ok((id, v.str("url")))
}

/// Upload an image to the backend's material store: `(file id, CDN URL)`.
pub async fn web_image(
  c: &Client,
  home: &Home,
  path: &Path,
  scene: &str,
) -> Result<(String, String)> {
  let image = Image::read(path).await?;
  let token = c.web_token().await?;
  let ticket = home
    .ticket
    .clone()
    .ok_or_else(|| Error::upstream("the backend home page has no upload ticket"))?;
  let now = jiff::Timestamp::now();
  let size = image.data.len().to_string();
  let parts = vec![
    Part::text("type", image.mime),
    Part::text("id", now.as_millisecond().to_string()),
    Part::text("name", image.name.clone()),
    Part::text("lastModifiedDate", now.to_string()),
    Part::text("size", size),
    Part::file("file", image.data, &image.name, image.mime),
  ];
  let v = c
    .ctx
    .http
    .post(format!("{MP}/cgi-bin/filetransfer"))
    .queries([
      ("action", "upload_material".to_owned()),
      ("f", "json".into()),
      ("scene", scene.into()),
      ("writetype", "doublewrite".into()),
      ("groupid", "1".into()),
      ("ticket_id", home.user_name.clone()),
      ("ticket", ticket),
      (
        "svr_time",
        home
          .time
          .clone()
          .unwrap_or_else(|| now.as_second().to_string()),
      ),
      ("token", token.clone()),
      ("lang", "zh_CN".into()),
      ("seq", now.as_millisecond().to_string()),
      ("t", format!("{}", rand::random::<f64>())),
    ])
    .header("origin", MP)
    .header(
      "referer",
      format!("{MP}/cgi-bin/home?t=home/index&lang=zh_CN&token={token}"),
    )
    .multipart(parts)
    .timeout(std::time::Duration::from_secs(120))
    .value()
    .await?;
  let v = crate::web::check(v)?;
  match (v.str("content"), v.str("cdn_url")) {
    (Some(id), Some(url)) => Ok((id, url)),
    _ => Err(Error::upstream(format!("the upload returned no file: {v}"))),
  }
}
