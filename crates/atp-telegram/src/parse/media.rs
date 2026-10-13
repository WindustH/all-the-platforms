//! Message media: what can be downloaded, and its file extension.

use atp_core::{Media, MediaKind};
use grammers_client::media::Media as TgMedia;

/// What a message carries, as one media entry addressed by `tg-media:`.
pub fn media_item(media: &TgMedia, post_id: &str) -> Option<Media> {
  let url = format!("tg-media:{post_id}");
  let item = match media {
    TgMedia::Photo(p) => {
      let mut m = Media::new(MediaKind::Image, url);
      m.size = p.size().map(|s| s as u64);
      m
    }
    TgMedia::Sticker(s) => {
      let mut m = Media::new(MediaKind::Image, url);
      m.alt = Some(s.emoji().to_owned());
      m
    }
    TgMedia::Document(d) => {
      let mime = d.mime_type().unwrap_or_default();
      let kind = if d.is_animated() || mime == "image/gif" {
        MediaKind::Gif
      } else if mime.starts_with("video/") {
        MediaKind::Video
      } else if mime.starts_with("audio/") {
        MediaKind::Audio
      } else if mime.starts_with("image/") {
        MediaKind::Image
      } else {
        MediaKind::File
      };
      let mut m = Media::new(kind, url);
      m.size = d.size().map(|s| s as u64);
      m.duration = d.duration();
      if let Some((w, h)) = d.resolution() {
        m.width = Some(w as u32);
        m.height = Some(h as u32);
      }
      m.alt = d.name().filter(|n| !n.is_empty()).map(str::to_owned);
      m
    }
    _ => return None,
  };
  Some(item)
}

/// File extension for a downloaded media.
pub fn extension(media: &TgMedia) -> String {
  match media {
    TgMedia::Photo(_) => "jpg".into(),
    TgMedia::Sticker(s) => match s.document.mime_type() {
      Some("application/x-tgsticker") => "tgs".into(),
      Some("video/webm") => "webm".into(),
      _ => "webp".into(),
    },
    TgMedia::Document(d) => {
      if let Some(ext) = d
        .name()
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 8)
      {
        return ext;
      }
      match d.mime_type().unwrap_or_default() {
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/webm" => "webm",
        "audio/ogg" => "ogg",
        "audio/mpeg" => "mp3",
        "audio/mp4" | "audio/x-m4a" => "m4a",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "application/pdf" => "pdf",
        "application/zip" => "zip",
        _ => "bin",
      }
      .into()
    }
    _ => "bin".into(),
  }
}
