//! Pictures for posts and comments, uploaded as the web app does: an
//! upload token from `upload.ruguoapp.com/token` (bucket `jike`, type
//! `PIC`), then the file to Qiniu (`upload.qiniup.com`), which answers the
//! key a post's `pictureKeys` names. The web app takes PNG and JPEG.

use std::path::Path;

use atp_core::file::Upload;
use atp_core::http::Part;
use atp_core::{Error, Result, ValueExt};

use crate::api::Api;

const TOKEN: &str = "https://upload.ruguoapp.com/token";
const QINIU: &str = "https://upload.qiniup.com";

/// Upload a picture; its key.
pub async fn picture(api: &Api, path: &Path) -> Result<String> {
  let file = Upload::read(path).await?;
  if !matches!(file.mime, "image/png" | "image/jpeg") {
    return Err(Error::input(format!(
      "{}: Jike takes PNG and JPEG pictures",
      path.display()
    )));
  }
  let params = [
    ("bucket", "jike".to_owned()),
    ("uploadType", "PIC".to_owned()),
  ];
  let v = api.get(TOKEN, &params).await?;
  let token = v
    .str("uptoken")
    .ok_or_else(|| Error::upstream("Jike gave no upload token"))?;
  let parts = vec![
    Part::text("token", token),
    Part::file("file", file.data, &file.name, file.mime),
  ];
  let answer = api
    .ctx
    .http
    .post(QINIU)
    .no_cookies()
    .multipart(parts)
    .send()
    .await?
    .check()?
    .value()?;
  answer
    .str("key")
    .ok_or_else(|| Error::upstream("the picture upload answered no key"))
}
