//! Fetching one media file: whole, in byte ranges by header, or in ranges
//! by URL parameter, with a progress bar.

use std::path::Path;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::AsyncWriteExt;

use crate::ctx::Ctx;
use crate::error::{Error, Result};
use crate::model::Media;

/// Bytes per ranged request: large files come as a series of these, each with
/// its own timeout and retries (some CDNs also throttle or refuse unranged
/// downloads of big files).
pub(super) const CHUNK: u64 = 8 << 20;

pub(super) const CHUNK_TIMEOUT: Duration = Duration::from_secs(120);

/// How to fetch one file: its size and how its host takes byte ranges.
#[derive(Clone, Copy)]
pub(super) struct Source<'a> {
  url: &'a str,
  size: Option<u64>,
  range_param: Option<&'a str>,
  headers: &'a [(String, String)],
}

impl<'a> Source<'a> {
  pub(super) fn video(m: &'a Media) -> Self {
    Self {
      url: &m.url,
      size: m.size,
      range_param: m.range_param.as_deref(),
      headers: &m.headers,
    }
  }

  pub(super) fn audio(m: &'a Media) -> Option<Self> {
    let url = m.audio_url.as_deref()?;
    Some(Self {
      url,
      size: m.audio_size,
      range_param: m.range_param.as_deref(),
      headers: &m.headers,
    })
  }
}

pub(super) async fn fetch(ctx: &Ctx, referer: &str, src: Source<'_>, path: &Path) -> Result<u64> {
  let label = path
    .file_name()
    .map(|f| f.to_string_lossy().into_owned())
    .unwrap_or_default();
  let bar = ProgressBar::new(0);
  bar.set_style(
    ProgressStyle::with_template(
      "{msg:40!} {bytes:>10}/{total_bytes:10} {bar:24} {bytes_per_sec:>10}",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar()),
  );
  bar.set_message(label);
  let result = match (src.range_param, src.size) {
    (Some(param), Some(size)) => fetch_by_param(ctx, referer, &src, param, size, path, &bar).await,
    _ => fetch_ranged(ctx, referer, &src, path, &bar).await,
  };
  bar.finish_and_clear();
  result
}

/// The media's own headers on a download request.
pub(super) fn with_headers<'a>(
  mut req: crate::http::Req<'a>,
  headers: &[(String, String)],
) -> crate::http::Req<'a> {
  for (name, value) in headers {
    req = req.header(name, value);
  }
  req
}

/// Chunks as `url&param=a-b`, for hosts (googlevideo) that take ranges in the
/// URL and answer a `Range` header with something else.
pub(super) async fn fetch_by_param(
  ctx: &Ctx,
  referer: &str,
  src: &Source<'_>,
  param: &str,
  size: u64,
  path: &Path,
  bar: &ProgressBar,
) -> Result<u64> {
  bar.set_length(size);
  let url = src.url;
  let sep = if url.contains('?') { '&' } else { '?' };
  let mut file = tokio::fs::File::create(path).await?;
  let mut done = 0;
  while done < size {
    let to = (done + CHUNK).min(size) - 1;
    let got = with_headers(
      ctx.http.get(format!("{url}{sep}{param}={done}-{to}")),
      src.headers,
    )
    .header("referer", referer)
    .header("accept", "*/*")
    .no_cookies()
    .no_throttle()
    .retries(2)
    .timeout(CHUNK_TIMEOUT)
    .append_to(&mut file, |n| bar.inc(n))
    .await?;
    if got.bytes == 0 {
      return Err(Error::network(format!(
        "the server sent nothing for bytes {done}-{to}"
      )));
    }
    done += got.bytes;
  }
  file.flush().await?;
  Ok(done)
}

/// Ask for the first chunk; a `206` with the file size continues chunk by
/// chunk, a `200` (ranges ignored) already carried the whole file.
pub(super) async fn fetch_ranged(
  ctx: &Ctx,
  referer: &str,
  src: &Source<'_>,
  path: &Path,
  bar: &ProgressBar,
) -> Result<u64> {
  let url = src.url;
  let chunk = |from: u64, to: u64| {
    with_headers(ctx.http.get(url), src.headers)
      .header("referer", referer)
      .header("accept", "*/*")
      .header("range", format!("bytes={from}-{to}"))
      .no_cookies()
      .no_throttle()
      .retries(2)
      .timeout(CHUNK_TIMEOUT)
  };
  let mut file = tokio::fs::File::create(path).await?;
  let first = chunk(0, CHUNK - 1)
    .append_to(&mut file, |n| bar.inc(n))
    .await?;
  let mut done = first.bytes;
  if first.status == wreq::StatusCode::PARTIAL_CONTENT
    && let Some(total) = first.range_total()
  {
    bar.set_length(total);
    while done < total {
      let to = (done + CHUNK).min(total) - 1;
      let got = chunk(done, to).append_to(&mut file, |n| bar.inc(n)).await?;
      if got.bytes == 0 {
        return Err(Error::network(format!(
          "the server sent nothing for bytes {done}-{to}"
        )));
      }
      done += got.bytes;
    }
  }
  file.flush().await?;
  Ok(done)
}
