//! Pictures for threads and replies: uploaded in chunks, then written into
//! the text as `#(pic,…)` tags.

use std::path::Path;

use md5::{Digest, Md5};
use social_core::file::Image;
use social_core::http::Part;
use social_core::{Error, Result, Value, ValueExt};

use crate::api::{Api, Params, WRITE};

const CHUNK: usize = 512_000;

/// The text with uploaded pictures appended as `#(pic,id,w,h)` markers.
pub async fn with_images(
  api: &Api,
  text: &str,
  images: &[impl AsRef<Path>],
  forum: &str,
) -> Result<String> {
  let mut out = text.to_owned();
  for path in images {
    let marker = upload(api, path.as_ref(), forum).await?;
    if !out.is_empty() {
      out.push('\n');
    }
    out += &marker;
  }
  if out.trim().is_empty() {
    return Err(Error::input("nothing to send: give a text or --image"));
  }
  Ok(out)
}

/// Upload a picture in 512 KB chunks, as the app does.
pub async fn upload(api: &Api, path: &Path, forum: &str) -> Result<String> {
  let image = Image::read(path).await?;
  let (width, height) = image.size();
  let size = image.data.len();
  let md5: String = Md5::digest(&image.data)
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect();
  let chunks: Vec<&[u8]> = image.data.chunks(CHUNK).collect();
  let mut last = Value::Null;
  for (i, chunk) in chunks.iter().enumerate() {
    let finish = i + 1 == chunks.len();
    let params: Params = vec![
      ("alt", "json".into()),
      ("chunkNo", (i + 1).to_string()),
      ("forum_name", forum.to_owned()),
      ("groupId", "1".into()),
      ("height", height.to_string()),
      ("isFinish", if finish { "1" } else { "0" }.into()),
      ("is_bjh", "0".into()),
      ("pic_water_type", "0".into()),
      ("resourceId", format!("{md5}{CHUNK}")),
      ("saveOrigin", "1".into()),
      ("size", size.to_string()),
      ("small_flow_fname", forum.to_owned()),
      ("width", width.to_string()),
    ];
    let part = Part::file("chunk", chunk.to_vec(), "file", "application/octet-stream");
    last = api
      .call_multipart("/c/s/uploadPicture", params, part, WRITE)
      .await?;
  }
  let id = last
    .str("picId")
    .ok_or_else(|| Error::upstream("picture upload returned no picId"))?;
  let pic_width = last
    .str("picInfo.originPic.width")
    .unwrap_or_else(|| width.to_string());
  let pic_height = last
    .str("picInfo.originPic.height")
    .unwrap_or_else(|| height.to_string());
  Ok(format!("#(pic,{id},{pic_width},{pic_height})"))
}
