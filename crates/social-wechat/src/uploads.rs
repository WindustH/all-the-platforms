//! Files for messages go to the c2c CDN first: `getuploadurl` hands out an
//! upload parameter, the file is uploaded AES-encrypted, and the CDN answers
//! with the parameter the message carries (`x-encrypted-param`).

use std::path::Path;
use std::time::Duration;

use md5::{Digest, Md5};
use social_core::file::Upload;
use social_core::{Error, Result, Value, ValueExt, json};
use url::form_urlencoded::byte_serialize;

use crate::api::{Account, Api, CDN};
use crate::crypto;

/// Kinds of upload, and the message item each becomes.
#[derive(Clone, Copy)]
enum Kind {
  Image,
  Video,
  File,
}

impl Kind {
  fn of(file: &Upload) -> Self {
    match file.mime {
      "image/jpeg" | "image/png" | "image/gif" | "image/webp" => Kind::Image,
      "video/mp4" | "video/quicktime" => Kind::Video,
      _ => Kind::File,
    }
  }

  /// `media_type` of `getuploadurl`.
  fn media_type(self) -> i64 {
    match self {
      Kind::Image => 1,
      Kind::Video => 2,
      Kind::File => 3,
    }
  }
}

/// Upload a file for `chat`; the message item that sends it.
pub async fn upload(api: &Api, account: &Account, chat: &str, path: &Path) -> Result<Value> {
  let file = Upload::read(path).await?;
  let kind = Kind::of(&file);
  let key: [u8; 16] = rand::random();
  let file_key = crypto::hex(&rand::random::<[u8; 16]>());
  let cipher = crypto::encrypt(&key, &file.data);
  let body = json!({
    "filekey": file_key,
    "media_type": kind.media_type(),
    "to_user_id": chat,
    "rawsize": file.data.len(),
    "rawfilemd5": crypto::hex(&Md5::digest(&file.data)),
    "filesize": crypto::padded_len(file.data.len()),
    "no_need_thumb": true,
    "aeskey": crypto::hex(&key),
  });
  let permit = api.call(account, "ilink/bot/getuploadurl", body).await?;
  let url = upload_url(&permit, &file_key)?;
  let param = put(api, &url, cipher.clone(), path).await?;
  let media = json!({
    "encrypt_query_param": param,
    "aes_key": crypto::key_for_message(&key),
    "encrypt_type": 1,
  });
  Ok(item(kind, media, &file, cipher.len()))
}

fn upload_url(permit: &Value, file_key: &str) -> Result<String> {
  let encode = |s: &str| byte_serialize(s.as_bytes()).collect::<String>();
  match (permit.str("upload_full_url"), permit.str("upload_param")) {
    (Some(full), _) => Ok(full),
    (None, Some(param)) => Ok(format!(
      "{CDN}/upload?encrypted_query_param={}&filekey={}",
      encode(&param),
      encode(file_key)
    )),
    (None, None) => Err(Error::upstream("getuploadurl gave no upload address")),
  }
}

/// Upload the encrypted bytes; the parameter the CDN confirms them with.
async fn put(api: &Api, url: &str, cipher: Vec<u8>, path: &Path) -> Result<String> {
  let resp = api
    .ctx
    .http
    .post(url)
    .bytes(cipher, "application/octet-stream")
    .no_cookies()
    .retries(2)
    .timeout(Duration::from_secs(300))
    .send()
    .await?;
  if !resp.status.is_success() {
    let why = resp
      .header("x-error-message")
      .map(str::to_owned)
      .unwrap_or_else(|| resp.text());
    return Err(Error::upstream(format!(
      "upload of {} failed: HTTP {} {why}",
      path.display(),
      resp.status
    )));
  }
  resp
    .header("x-encrypted-param")
    .map(str::to_owned)
    .ok_or_else(|| Error::upstream("the CDN did not confirm the upload (no x-encrypted-param)"))
}

fn item(kind: Kind, media: Value, file: &Upload, encrypted_len: usize) -> Value {
  match kind {
    Kind::Image => {
      json!({ "type": 2, "image_item": { "media": media, "mid_size": encrypted_len } })
    }
    Kind::Video => {
      json!({ "type": 5, "video_item": { "media": media, "video_size": encrypted_len } })
    }
    Kind::File => json!({
      "type": 4,
      "file_item": {
        "media": media,
        "file_name": file.name,
        "len": file.data.len().to_string(),
      },
    }),
  }
}
