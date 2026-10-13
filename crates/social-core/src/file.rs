//! Local files: format detection by magic bytes, and files to upload
//! ([`Upload`] for anything, [`Image`] where only images are accepted).

use std::path::Path;

use crate::error::{Error, Result};

/// File extension for the format the first bytes of a file reveal.
pub fn sniff(head: &[u8]) -> Option<&'static str> {
  let at = |range: std::ops::Range<usize>| head.get(range).unwrap_or_default();
  if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
    return Some("jpg");
  }
  if head.starts_with(b"\x89PNG") {
    return Some("png");
  }
  if head.starts_with(b"GIF8") {
    return Some("gif");
  }
  if head.starts_with(b"RIFF") && at(8..12) == b"WEBP" {
    return Some("webp");
  }
  if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
    return Some("webm");
  }
  if head.starts_with(b"FLV") {
    return Some("flv");
  }
  if at(4..8) == b"ftyp" {
    return Some(match at(8..12) {
      b"heic" | b"heix" | b"heim" | b"heis" | b"mif1" | b"msf1" => "heic",
      b"avif" | b"avis" => "avif",
      b"M4A " => "m4a",
      b"qt  " => "mov",
      _ => "mp4",
    });
  }
  None
}

/// An image read from disk, ready to upload.
#[derive(Debug, Clone)]
pub struct Image {
  pub data: Vec<u8>,
  /// `image/jpeg`, `image/png`, `image/gif`, `image/webp`, `image/heic` or `image/avif`.
  pub mime: &'static str,
  /// File name to send along, with an extension matching `mime`.
  pub name: String,
}

impl Image {
  /// Read `path` and detect its format from its content, not its extension.
  pub async fn read(path: &Path) -> Result<Self> {
    let data = tokio::fs::read(path)
      .await
      .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
    let ext = sniff(&data).unwrap_or_default();
    let mime = mime_for(ext);
    if !matches!(ext, "jpg" | "png" | "gif" | "webp" | "heic" | "avif") {
      return Err(Error::input(format!(
        "not a supported image: {}",
        path.display()
      )));
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    Ok(Self {
      data,
      mime,
      name: format!("{stem}.{ext}"),
    })
  }

  /// Width and height in pixels, `(0, 0)` when the header is unreadable.
  pub fn size(&self) -> (usize, usize) {
    imagesize::blob_size(&self.data)
      .map(|s| (s.width, s.height))
      .unwrap_or((0, 0))
  }

  /// Extension matching the detected format.
  pub fn ext(&self) -> &str {
    self.name.rsplit_once('.').map(|(_, e)| e).unwrap_or("jpg")
  }
}

/// MIME type for a file extension (lower case, without the dot).
pub fn mime_for(ext: &str) -> &'static str {
  match ext {
    "jpg" | "jpeg" => "image/jpeg",
    "png" => "image/png",
    "gif" => "image/gif",
    "webp" => "image/webp",
    "heic" => "image/heic",
    "avif" => "image/avif",
    "bmp" => "image/bmp",
    "svg" => "image/svg+xml",
    "mp4" | "m4v" => "video/mp4",
    "mov" => "video/quicktime",
    "webm" => "video/webm",
    "mkv" => "video/x-matroska",
    "flv" => "video/x-flv",
    "mp3" => "audio/mpeg",
    "m4a" => "audio/mp4",
    "aac" => "audio/aac",
    "ogg" | "oga" | "opus" => "audio/ogg",
    "wav" => "audio/wav",
    "amr" => "audio/amr",
    "flac" => "audio/flac",
    "pdf" => "application/pdf",
    "zip" => "application/zip",
    "txt" | "md" | "log" => "text/plain",
    "csv" => "text/csv",
    "json" => "application/json",
    "html" | "htm" => "text/html",
    "doc" => "application/msword",
    "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "xls" => "application/vnd.ms-excel",
    "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "ppt" => "application/vnd.ms-powerpoint",
    "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    _ => "application/octet-stream",
  }
}

/// Any file read from disk, ready to upload: its bytes, MIME type (from the
/// content where it tells, else the extension) and file name.
#[derive(Debug, Clone)]
pub struct Upload {
  pub data: Vec<u8>,
  pub mime: &'static str,
  pub name: String,
}

impl Upload {
  pub async fn read(path: &Path) -> Result<Self> {
    let data = tokio::fs::read(path)
      .await
      .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
    let ext = path
      .extension()
      .and_then(|e| e.to_str())
      .map(str::to_ascii_lowercase)
      .unwrap_or_default();
    let mime = match sniff(&data) {
      Some(kind) => mime_for(kind),
      None => mime_for(&ext),
    };
    let name = path
      .file_name()
      .and_then(|n| n.to_str())
      .unwrap_or("file")
      .to_owned();
    Ok(Self { data, mime, name })
  }

  /// What kind of media the file is, by its MIME type.
  pub fn kind(&self) -> crate::model::MediaKind {
    use crate::model::MediaKind;
    match self.mime.split('/').next() {
      Some("image") if self.mime == "image/gif" => MediaKind::Gif,
      Some("image") => MediaKind::Image,
      Some("video") => MediaKind::Video,
      Some("audio") => MediaKind::Audio,
      _ => MediaKind::File,
    }
  }

  pub fn is_image(&self) -> bool {
    self.mime.starts_with("image/")
  }
}
