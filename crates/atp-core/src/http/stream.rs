//! Responses read as they arrive: long-lived feeds, and bodies streamed
//! onto the end of a file.

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use super::{Fetched, Req, StatusCode, status_error};
use crate::error::{Error, Result};

impl Req<'_> {
  /// Read a response as it arrives, handing each chunk to `f`, until the
  /// server ends it or `f` returns `false` (server-sent events and other
  /// long-lived feeds; give the request a long `timeout`).
  pub async fn each_chunk(self, mut f: impl FnMut(&[u8]) -> bool) -> Result<()> {
    let resp = self.execute().await?;
    let status = resp.status();
    if !status.is_success() {
      let body = resp
        .bytes()
        .await
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
      return Err(status_error(status, &body));
    }
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
      let chunk = chunk.map_err(|e| Error::network(e.to_string()))?;
      if !f(&chunk) {
        break;
      }
    }
    Ok(())
  }

  /// Stream a successful response's body onto the end of `file`, calling
  /// `progress(bytes)` per chunk; the status and headers come back with the count.
  pub async fn append_to(
    self,
    file: &mut tokio::fs::File,
    progress: impl FnMut(u64),
  ) -> Result<Fetched> {
    self.append_checked(file, false, progress).await
  }

  /// [`append_to`](Self::append_to) for a range that continues a partial
  /// file: an answer other than `206 Partial Content` (the host ignored the
  /// range) fails before a byte is written.
  pub async fn append_range_to(
    self,
    file: &mut tokio::fs::File,
    progress: impl FnMut(u64),
  ) -> Result<Fetched> {
    self.append_checked(file, true, progress).await
  }

  async fn append_checked(
    self,
    file: &mut tokio::fs::File,
    partial: bool,
    mut progress: impl FnMut(u64),
  ) -> Result<Fetched> {
    let resp = self.execute().await?;
    let status = resp.status();
    if !status.is_success() {
      return Err(status_error(status, ""));
    }
    if partial && status != StatusCode::PARTIAL_CONTENT {
      return Err(Error::upstream(format!(
        "the host ignored the byte range (answered {status})"
      )));
    }
    let headers = resp.headers().clone();
    let mut bytes = 0u64;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
      let chunk = chunk.map_err(|e| Error::network(e.to_string()))?;
      file.write_all(&chunk).await?;
      bytes += chunk.len() as u64;
      progress(chunk.len() as u64);
    }
    Ok(Fetched {
      status,
      headers,
      bytes,
    })
  }
}
