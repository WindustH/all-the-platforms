//! Image uploads: registered by hash, stored on Zhihu's OSS and polled
//! until processed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use md5::{Digest, Md5};
use social_core::http::Method;
use social_core::{Error, Result, Value, ValueExt, json};

use crate::api::{Api, MOBILE};
use crate::sign;

const OSS: &str = "https://zhihu-pics-upload.zhimg.com";

/// An uploaded image, as the editors embed it.
pub struct Image {
  pub src: String,
  pub original: String,
  pub watermark: String,
  pub watermark_src: String,
  pub width: usize,
  pub height: usize,
}

async fn upload(api: &Api, path: &Path, source: &str) -> Result<Image> {
  let file = social_core::file::Image::read(path).await?;
  let (width, height) = file.size();
  let data = file.data;
  let hash = format!("{:x}", Md5::digest(&data));
  let mut v = register(api, &hash, source).await?;
  match v.i64("upload_file.state") {
    Some(1) => {} // already known to Zhihu
    Some(2) => {
      put_object(api, &v, data, file.mime).await?;
      // The id handed out before the upload stays `init` forever; registering
      // again yields a new id (state 1) whose processing does complete.
      v = register(api, &hash, source).await?;
    }
    other => return Err(Error::upstream(format!("unexpected image state {other:?}"))),
  }
  let image_id = v
    .str("upload_file.image_id")
    .ok_or_else(|| Error::upstream("image registration returned no image_id"))?;
  let info = poll_image(api, &image_id).await?;
  let src = info.str("src").unwrap_or_default();
  Ok(Image {
    original: info.str("original_src").unwrap_or_else(|| src.clone()),
    src,
    watermark: info.str("watermark").unwrap_or_else(|| "watermark".into()),
    watermark_src: info.str("watermark_src").unwrap_or_default(),
    width,
    height,
  })
}

/// Register an image by content hash: its id, and an upload token when Zhihu lacks it.
async fn register(api: &Api, hash: &str, source: &str) -> Result<Value> {
  api
    .call(
      api
        .post(&format!("{MOBILE}/images"))
        .json(&json!({ "image_hash": hash, "source": source })),
    )
    .await
}

/// Upload the bytes to Aliyun OSS with the STS token from the registration.
async fn put_object(api: &Api, reg: &Value, data: Vec<u8>, content_type: &str) -> Result<()> {
  let key = reg
    .str("upload_file.object_key")
    .ok_or_else(|| Error::upstream("image registration returned no object_key"))?;
  let token = reg.at("upload_token");
  let field = |name: &str| {
    token
      .str(name)
      .ok_or_else(|| Error::upstream(format!("upload token lacks {name}")))
  };
  let (id, secret, sts) = (
    field("access_id")?,
    field("access_key")?,
    field("access_token")?,
  );
  let date = jiff::Timestamp::now()
    .strftime("%a, %d %b %Y %H:%M:%S GMT")
    .to_string();
  let auth = sign::oss_authorization(&id, &secret, &sts, content_type, &date, &key);
  let resp = api
    .ctx
    .http
    .request(Method::PUT, format!("{OSS}/{key}"))
    .header("date", &date)
    .header("x-oss-security-token", &sts)
    .header("authorization", auth)
    .bytes(data, content_type)
    .no_cookies()
    .retries(1)
    .send()
    .await?;
  resp.check().map(drop)
}

async fn poll_image(api: &Api, image_id: &str) -> Result<Value> {
  // Processing can take a minute or more; poll with a growing interval (~2 min in all).
  let mut status = String::new();
  for attempt in 0..30u64 {
    let v = api
      .call(api.get(&format!("{MOBILE}/images/{image_id}")))
      .await?;
    status = v.str("status").unwrap_or_default();
    match status.as_str() {
      "success" => return Ok(v),
      "fail" | "failed" | "error" => {
        return Err(Error::upstream(format!(
          "Zhihu could not process the image ({status})"
        )));
      }
      _ => tokio::time::sleep(Duration::from_secs((2 + attempt / 4).min(6))).await,
    }
  }
  Err(Error::upstream(format!(
    "image processing timed out (last status `{status}`)"
  )))
}

pub async fn upload_all(api: &Api, paths: &[PathBuf], source: &str) -> Result<Vec<Image>> {
  let mut images = Vec::with_capacity(paths.len());
  for path in paths {
    images.push(upload(api, path, source).await?);
  }
  Ok(images)
}

impl Image {
  pub fn html(&self) -> String {
    format!(
      r#"<img src="{}" data-caption="" data-size="normal" data-rawwidth="{}" data-rawheight="{}" data-watermark="{}" data-original-src="{}" data-watermark-src="{}" data-private-watermark-src=""/>"#,
      self.src, self.width, self.height, self.watermark, self.original, self.watermark_src
    )
  }
}
