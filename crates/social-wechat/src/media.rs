//! Downloading received media: the encrypted file from the c2c CDN,
//! decrypted with the key the message carries.

use std::path::{Path, PathBuf};
use std::time::Duration;

use social_core::{Ctx, Error, Media, Result, Value, ValueExt};
use url::form_urlencoded::byte_serialize;

use crate::api::{Account, Api, CDN};
use crate::parse::SCHEME;
use crate::{archive, crypto, refs};

pub async fn save(
  api: &Api,
  account: &Account,
  media: &Media,
  dir: &Path,
  name: &str,
) -> Result<PathBuf> {
  let ctx = &api.ctx;
  let (item, field) = item(ctx, account, &media.url)?;
  let v = item.at(field);
  let m = v.at("media");
  let url = match m.str("full_url") {
    Some(full) => full,
    None => {
      let param = m
        .str("encrypt_query_param")
        .ok_or_else(|| Error::upstream("the message has no media reference"))?;
      format!(
        "{CDN}/download?encrypted_query_param={}",
        byte_serialize(param.as_bytes()).collect::<String>()
      )
    }
  };
  // Images carry their key as hex beside the media; others only as base64.
  let key = v
    .str("aeskey")
    .and_then(|k| crypto::key_from_hex(&k))
    .or_else(|| m.str("aes_key").and_then(|k| crypto::key_from_base64(&k)));
  let resp = ctx
    .http
    .get(url)
    .no_cookies()
    .timeout(Duration::from_secs(300))
    .send()
    .await?
    .check()?;
  let data = match key {
    Some(k) => crypto::decrypt(&k, &resp.body)?,
    None => resp.body,
  };
  let ext = extension(&item, v, &data);
  tokio::fs::create_dir_all(dir).await?;
  let path = dir.join(format!("{name}.{ext}"));
  tokio::fs::write(&path, data).await?;
  Ok(path)
}

/// The message item a `wechat-media:CHAT/MESSAGE/INDEX` URL points at.
fn item(ctx: &Ctx, account: &Account, url: &str) -> Result<(Value, &'static str)> {
  let rest = url
    .strip_prefix(SCHEME)
    .ok_or_else(|| Error::input(format!("not a WeChat media reference: {url}")))?;
  let (message, index) = rest
    .rsplit_once('/')
    .ok_or_else(|| Error::input(format!("not a WeChat media reference: {url}")))?;
  let (chat, id) = refs::message(message)?;
  let index: usize = index
    .parse()
    .map_err(|_| Error::input(format!("not a WeChat media reference: {url}")))?;
  let m = archive::messages(ctx, account, &chat)
    .into_iter()
    .find(|m| [m.str("message_id"), m.str("client_id")].contains(&Some(id.clone())))
    .ok_or_else(|| Error::not_found(format!("message {message} is not on record here")))?;
  let item = m
    .list("item_list")
    .get(index)
    .cloned()
    .ok_or_else(|| Error::not_found(format!("message {message} has no item {index}")))?;
  let field = match item.i64("type") {
    Some(2) => "image_item",
    Some(3) => "voice_item",
    Some(4) => "file_item",
    Some(5) => "video_item",
    _ => {
      return Err(Error::not_found(format!(
        "item {index} of {message} is not media"
      )));
    }
  };
  Ok((item, field))
}

fn extension(item: &Value, v: &Value, data: &[u8]) -> String {
  match item.i64("type") {
    Some(4) => v
      .str("file_name")
      .and_then(|n| n.rsplit_once('.').map(|(_, e)| e.to_owned()))
      .filter(|e| !e.is_empty() && e.len() <= 8)
      .unwrap_or_else(|| "bin".into()),
    // Voice: 5 amr, 6 silk (the usual), 7 mp3, 8 ogg-speex.
    Some(3) => match v.i64("encode_type") {
      Some(5) => "amr",
      Some(7) => "mp3",
      Some(8) => "ogg",
      _ => "silk",
    }
    .into(),
    _ => social_core::file::sniff(data).unwrap_or("bin").into(),
  }
}
