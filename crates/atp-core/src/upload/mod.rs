//! Uploads too big to hold in memory: a file on disk hashed in one pass
//! ([`Hashes`]) and sent part by part ([`Parts`]), each part with its own
//! retries, under a progress bar. What was sent is kept in the private
//! cache, so a later run of the same upload continues where this one stopped.
//! Small files that fit in memory use [`crate::file::Upload`].

mod hashes;
mod midway;

use std::path::{Path, PathBuf};
use std::time::Duration;

use md5::{Digest, Md5};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::ctx::Ctx;
use crate::error::{Error, ErrorCode, Result};
use crate::file::{mime_for, sniff};
use crate::output::{note, progress};
use crate::task;

pub use self::hashes::{BLOCK, HEAD, Hashes};
pub use self::midway::Sha1Midway;

/// Calls one part gets before the upload gives up.
const PART_TRIES: u32 = 3;

/// A local file to upload in parts.
#[derive(Debug, Clone)]
pub struct LocalFile {
  /// The full path.
  pub path: PathBuf,
  /// The name it gets on the drive.
  pub name: String,
  pub size: u64,
  /// By content where the first bytes tell, else by extension.
  pub mime: &'static str,
  /// Last modification, Unix milliseconds (0 when unknown).
  pub modified_ms: i64,
}

impl LocalFile {
  pub async fn open(path: &Path) -> Result<Self> {
    let fail = |e: std::io::Error| Error::input(format!("cannot read {}: {e}", path.display()));
    let path = tokio::fs::canonicalize(path).await.map_err(fail)?;
    let meta = tokio::fs::metadata(&path).await.map_err(fail)?;
    if !meta.is_file() {
      return Err(Error::input(format!("not a file: {}", path.display())));
    }
    let mut head = [0u8; 16];
    let read = tokio::fs::File::open(&path)
      .await
      .map_err(fail)?
      .read(&mut head)
      .await;
    let ext = path
      .extension()
      .and_then(|e| e.to_str())
      .map(str::to_ascii_lowercase)
      .unwrap_or_default();
    let mime = match sniff(&head[..read.unwrap_or(0)]) {
      Some(kind) => mime_for(kind),
      None => mime_for(&ext),
    };
    let modified_ms = meta
      .modified()
      .ok()
      .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
      .map_or(0, |d| d.as_millis() as i64);
    let name = path
      .file_name()
      .map(|n| n.to_string_lossy().into_owned())
      .unwrap_or_else(|| "file".into());
    Ok(Self {
      size: meta.len(),
      path,
      name,
      mime,
      modified_ms,
    })
  }

  /// Digests of the whole file (one read of it).
  pub async fn hashes(&self) -> Result<Hashes> {
    Hashes::of(&self.path, self.size).await
  }

  /// How many parts of `part_size` bytes the file takes (one for an empty file).
  pub fn parts(&self, part_size: u64) -> u32 {
    self.size.div_ceil(part_size.max(1)).max(1) as u32
  }

  /// The bytes of part `number` (counting from 1).
  pub async fn part(&self, number: u32, part_size: u64) -> Result<Vec<u8>> {
    let start = u64::from(number.saturating_sub(1)) * part_size;
    let len = part_size.min(self.size.saturating_sub(start));
    let mut file = tokio::fs::File::open(&self.path).await?;
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let mut data = vec![0u8; len as usize];
    file.read_exact(&mut data).await?;
    Ok(data)
  }
}

/// What a parts upload keeps between runs: the drive's upload session
/// (upload id, task, signed fields, part size ...) and what each part sent
/// so far is known by (ETag, md5 ...), in order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resume<T> {
  pub session: T,
  pub sent: Vec<String>,
}

impl<T> Resume<T> {
  pub fn new(session: T) -> Self {
    Self {
      session,
      sent: Vec::new(),
    }
  }
}

/// One part on its way: its number (from 1), its bytes, and where SHA-1
/// stood before it when the upload tracks that ([`Parts::with_sha1`]).
pub struct Part {
  pub number: u32,
  pub data: Vec<u8>,
  pub sha1: Option<Sha1Midway>,
}

/// The parts upload of one file to one place.
pub struct Parts<'a> {
  ctx: &'a Ctx,
  file: &'a LocalFile,
  key: String,
  sha1: bool,
}

impl<'a> Parts<'a> {
  /// `target` names where the file goes (a folder id, a remote path), so
  /// the same file sent somewhere else is another upload.
  pub fn new(ctx: &'a Ctx, file: &'a LocalFile, target: &str) -> Self {
    let id = format!(
      "{}\n{}\n{}\n{target}",
      file.path.display(),
      file.size,
      file.modified_ms
    );
    Self {
      ctx,
      file,
      key: format!("upload-{:x}", Md5::digest(id.as_bytes())),
      sha1: false,
    }
  }

  /// Hand each part the SHA-1 state of everything before it (the part
  /// size must then be a multiple of 64 bytes).
  pub fn with_sha1(mut self) -> Self {
    self.sha1 = true;
    self
  }

  /// The state an earlier run kept, if it is younger than `ttl`.
  pub fn resume<T: DeserializeOwned>(&self, ttl: Duration) -> Option<Resume<T>> {
    self.ctx.store.cache_get(&self.key, ttl)
  }

  /// Keep the state for a later run.
  pub fn keep<T: Serialize>(&self, state: &Resume<T>) {
    self.ctx.store.cache_put(&self.key, state);
  }

  /// The upload is over (done, or it cannot continue): drop its state.
  pub fn forget(&self) {
    self.ctx.store.cache_remove(&self.key);
  }

  /// Send the parts not yet in `state.sent`, in order. `send(part)` uploads
  /// one part and returns what the drive knows it by. A part failing on the
  /// network or the server is tried again after a pause; the state is kept
  /// after every part.
  pub async fn send<T: Serialize>(
    &self,
    state: &mut Resume<T>,
    part_size: u64,
    mut send: impl AsyncFnMut(&Part) -> Result<String>,
  ) -> Result<()> {
    let count = self.file.parts(part_size);
    let first = state.sent.len() as u32 + 1;
    if first > 1 && first <= count {
      note(&format!("continuing at part {first} of {count}"));
    }
    let mut sha1 = self.sha1_before(first, part_size).await?;
    let bar = progress(&self.file.name);
    bar.set_length(self.file.size);
    bar.set_position((u64::from(first - 1) * part_size).min(self.file.size));
    for number in first..=count {
      let data = self.file.part(number, part_size).await?;
      let part = Part { number, data, sha1 };
      let result = task::retry(PART_TRIES, transient, async || send(&part).await).await;
      let tag = match result {
        Ok(tag) => tag,
        Err(e) => {
          bar.finish_and_clear();
          return Err(e);
        }
      };
      state.sent.push(tag);
      self.keep(state);
      bar.inc(part.data.len() as u64);
      if let Some(midway) = sha1.as_mut()
        && number < count
      {
        midway.update(&part.data)?;
      }
    }
    bar.finish_and_clear();
    Ok(())
  }

  /// Where SHA-1 stands before part `first`: the parts an earlier run sent
  /// are read again. `None` unless the upload tracks it.
  async fn sha1_before(&self, first: u32, part_size: u64) -> Result<Option<Sha1Midway>> {
    if !self.sha1 {
      return Ok(None);
    }
    let mut midway = Sha1Midway::default();
    for number in 1..first {
      midway.update(&self.file.part(number, part_size).await?)?;
    }
    Ok(Some(midway))
  }
}

/// Failures another try may get past: the network, rate limits, server errors.
fn transient(e: &Error) -> bool {
  match e.code {
    ErrorCode::NetworkError | ErrorCode::RateLimited => true,
    ErrorCode::UpstreamError => e.message.starts_with("HTTP 5"),
    _ => false,
  }
}
