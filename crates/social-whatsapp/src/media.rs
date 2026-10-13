//! Media of archived messages, downloaded and decrypted (no connection
//! needed); and the Cloud API's media by id.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use social_core::{Error, Media, Post, Result};
use whatsapp_rust::TokioRuntime;
use whatsapp_rust::download::{DownloadParams, MediaDownloader};
use whatsapp_rust::wacore::download::MediaType;

use crate::api::{Api, Mode};
use crate::archive::Archive;
use crate::chats;
use crate::personal::Personal;
use crate::refs;

/// The URL scheme of archived media: `wa-media:CHAT/MESSAGE`.
pub const SCHEME: &str = "wa-media:";

/// Cloud API: a media id from a webhook; personal: an archived message.
pub async fn media(api: &Api, post: &str) -> Result<(Post, Vec<Media>)> {
  if let Mode::Cloud(cloud) = api.mode() {
    let media = cloud.media(&api.ctx, post).await?;
    let p = Post {
      id: post.to_owned(),
      kind: "media".into(),
      ..Post::default()
    };
    return Ok((p, vec![media]));
  }
  let p = api
    .on_device("read", async |link| chats::read(link, post))
    .await?;
  let media = p.media.clone();
  if media.is_empty() {
    return Err(Error::not_found("this message has no media"));
  }
  Ok((p, media))
}

/// Download an archived message's media.
pub async fn save(personal: &Personal, url: &str, dir: &Path, name: &str) -> Result<PathBuf> {
  let reference = url.strip_prefix(SCHEME).unwrap_or(url);
  let (chat, id) = refs::message(reference)?;
  let mut archive = Archive::open(&personal.archive_dir);
  let Some(m) = archive.find_message(&chat, &id).and_then(|r| r.media) else {
    return Err(Error::not_found("this message has no archived media"));
  };
  let decode = |s: &str| {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
      .decode(s)
      .map_err(|_| Error::internal("corrupt media key in the archive"))
  };
  let kind = match m.kind.as_str() {
    "image" => MediaType::Image,
    "video" | "gif" => MediaType::Video,
    "audio" | "voice" => MediaType::Audio,
    "sticker" => MediaType::Sticker,
    _ => MediaType::Document,
  };
  let params = DownloadParams::encrypted(
    m.direct_path.clone(),
    &decode(&m.key)?,
    &decode(&m.sha256)?,
    &decode(&m.enc_sha256)?,
    m.length,
    kind,
  );
  let downloader = MediaDownloader::with_default_hosts(
    Arc::new(crate::proxy::http_client(personal.proxy.as_deref())),
    Arc::new(TokioRuntime),
  );
  let data = downloader.download(&params).await.map_err(|e| {
    Error::upstream(format!("media download failed: {e}"))
      .with_hint("old media expire on WhatsApp's servers after a while")
  })?;
  let ext = extension(&m.kind, m.mime.as_deref(), m.file_name.as_deref());
  let file_name = match (&m.file_name, m.kind.as_str()) {
    (Some(f), "document") => f.clone(),
    _ => format!("{name}.{ext}"),
  };
  let path = dir.join(file_name);
  std::fs::create_dir_all(dir)?;
  std::fs::write(&path, data)?;
  Ok(path)
}

fn extension(kind: &str, mime: Option<&str>, file_name: Option<&str>) -> String {
  if let Some(ext) = file_name.and_then(|f| f.rsplit_once('.')).map(|(_, e)| e) {
    return ext.to_owned();
  }
  let from_mime = mime
    .and_then(|m| m.split(';').next())
    .and_then(|m| m.split('/').nth(1))
    .map(|e| match e {
      "jpeg" => "jpg",
      "ogg" => "ogg",
      "mpeg" => "mp3",
      other => other,
    });
  from_mime
    .unwrap_or(match kind {
      "image" => "jpg",
      "video" | "gif" => "mp4",
      "voice" => "ogg",
      "audio" => "mp3",
      "sticker" => "webp",
      _ => "bin",
    })
    .to_owned()
}
