//! Download the media of a post: images, videos (merging split audio/video
//! with ffmpeg), audio-only extraction and ASR-ready WAV segments; and the
//! files and folders of a drive ([`files`]).

mod fetch;
mod ffmpeg;
mod files;

use tokio::io::AsyncReadExt;

use crate::ctx::Ctx;
use crate::error::{Error, Result};
use crate::model::{Downloaded, Media, MediaKind, Post};
use crate::output::note;
use crate::text::{file_stem, one_line};
use std::path::{Path, PathBuf};

use self::fetch::*;
use self::ffmpeg::*;
pub use self::files::{Link, files};

#[derive(Debug, Clone)]
pub struct DownloadOpts {
  pub dir: PathBuf,
  /// Keep only the audio track of videos.
  pub audio_only: bool,
  /// Split audio into WAV segments of this many seconds (16 kHz mono).
  pub split: Option<u32>,
}

/// `save` is the platform's [`Platform::save_media`](crate::Platform::save_media):
/// files it saves itself skip the HTTP fetch.
pub async fn download(
  ctx: &Ctx,
  referer: &str,
  post: &Post,
  media: &[Media],
  opts: &DownloadOpts,
  save: impl AsyncFn(&Media, &Path, &str) -> Option<Result<PathBuf>>,
) -> Result<Vec<Downloaded>> {
  let wanted: Vec<&Media> = media
    .iter()
    .filter(|m| !opts.audio_only || matches!(m.kind, MediaKind::Video | MediaKind::Audio))
    .collect();
  if wanted.is_empty() {
    return Err(Error::not_found("this post has no downloadable media"));
  }
  tokio::fs::create_dir_all(&opts.dir).await?;
  let stem = base_name(post);
  let mut saved = Vec::new();
  for (i, m) in wanted.iter().enumerate() {
    let name = if wanted.len() > 1 {
      format!("{stem}-{}", i + 1)
    } else {
      stem.clone()
    };
    let path = if let Some(saved) = save(m, &opts.dir, &name).await {
      saved?
    } else if opts.audio_only {
      audio(ctx, referer, m, &opts.dir, &name).await?
    } else {
      full(ctx, referer, m, &opts.dir, &name).await?
    };
    let kind = if opts.audio_only {
      MediaKind::Audio
    } else {
      m.kind
    };
    if let (Some(secs), true) = (
      opts.split,
      matches!(kind, MediaKind::Audio | MediaKind::Video),
    ) {
      for seg in split(&path, &opts.dir.join(&name), secs).await? {
        saved.push(downloaded(MediaKind::Audio, &seg).await);
      }
    }
    saved.push(downloaded(kind, &path).await);
  }
  Ok(saved)
}

fn base_name(post: &Post) -> String {
  let label = post
    .title
    .as_deref()
    .or(post.text.as_deref())
    .map(one_line)
    .unwrap_or_default();
  let label = file_stem(&label);
  if label.is_empty() {
    file_stem(&post.id)
  } else {
    format!("{label} [{}]", file_stem(&post.id))
  }
}

async fn downloaded(kind: MediaKind, path: &Path) -> Downloaded {
  let bytes = tokio::fs::metadata(path)
    .await
    .map(|m| m.len())
    .unwrap_or(0);
  Downloaded {
    kind,
    path: path.display().to_string(),
    bytes,
  }
}

fn extension(url: &str, kind: MediaKind) -> &'static str {
  let path = url.split(['?', '#']).next().unwrap_or(url);
  let last = path.rsplit('/').next().unwrap_or("");
  let ext = last
    .rsplit_once('.')
    .map(|(_, e)| e.to_ascii_lowercase())
    .unwrap_or_default();
  const KNOWN: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "heic", "avif", "mp4", "m4a", "mp3", "flv", "webm", "mov",
  ];
  match KNOWN.iter().find(|k| **k == ext) {
    Some(k) => k,
    None => match kind {
      MediaKind::Image => "jpg",
      MediaKind::Video | MediaKind::Gif => "mp4",
      MediaKind::Audio => "m4a",
      MediaKind::File => "bin",
    },
  }
}

/// Rename `path` when its magic bytes show another format than the extension
/// guessed from the URL (e.g. HEIC originals behind extension-less URLs).
async fn fix_extension(path: PathBuf) -> PathBuf {
  let mut head = [0u8; 16];
  let read = async {
    let mut f = tokio::fs::File::open(&path).await?;
    f.read(&mut head).await
  };
  let Ok(n) = read.await else { return path };
  let Some(ext) = crate::file::sniff(&head[..n]) else {
    return path;
  };
  let current = path.extension().and_then(|e| e.to_str()).unwrap_or("");
  if current.eq_ignore_ascii_case(ext) || (ext == "jpg" && current.eq_ignore_ascii_case("jpeg")) {
    return path;
  }
  let renamed = path.with_extension(ext);
  match tokio::fs::rename(&path, &renamed).await {
    Ok(()) => renamed,
    Err(_) => path,
  }
}

async fn full(ctx: &Ctx, referer: &str, m: &Media, dir: &Path, name: &str) -> Result<PathBuf> {
  let out = dir.join(format!("{name}.{}", extension(&m.url, m.kind)));
  let Some(audio_src) = Source::audio(m) else {
    fetch(ctx, referer, Source::video(m), &out).await?;
    return Ok(fix_extension(out).await);
  };
  let out = dir.join(format!("{name}.mp4"));
  let video = dir.join(format!("{name}.video.m4s"));
  let audio = dir.join(format!("{name}.audio.m4s"));
  fetch(ctx, referer, Source::video(m), &video).await?;
  fetch(ctx, referer, audio_src, &audio).await?;
  if !has_ffmpeg().await {
    note("ffmpeg not found: kept the separate video and audio tracks");
    return Ok(video);
  }
  ffmpeg(&[
    "-i".as_ref(),
    video.as_os_str(),
    "-i".as_ref(),
    audio.as_os_str(),
    "-c".as_ref(),
    "copy".as_ref(),
    out.as_os_str(),
  ])
  .await?;
  let _ = tokio::fs::remove_file(&video).await;
  let _ = tokio::fs::remove_file(&audio).await;
  Ok(out)
}

async fn audio(ctx: &Ctx, referer: &str, m: &Media, dir: &Path, name: &str) -> Result<PathBuf> {
  let out = dir.join(format!("{name}.m4a"));
  if let Some(src) = Source::audio(m).or((m.kind == MediaKind::Audio).then(|| Source::video(m))) {
    fetch(ctx, referer, src, &out).await?;
    return Ok(out);
  }
  let video = dir.join(format!("{name}.source.{}", extension(&m.url, m.kind)));
  fetch(ctx, referer, Source::video(m), &video).await?;
  require_ffmpeg().await?;
  ffmpeg(&[
    "-i".as_ref(),
    video.as_os_str(),
    "-vn".as_ref(),
    "-c:a".as_ref(),
    "aac".as_ref(),
    out.as_os_str(),
  ])
  .await?;
  let _ = tokio::fs::remove_file(&video).await;
  Ok(out)
}
