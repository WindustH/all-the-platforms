//! Files of a drive: saved under the server's names, folders as
//! directories, through `.part` files that a later run continues.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use indicatif::ProgressBar;
use tokio::io::AsyncWriteExt;

use super::fetch::{CHUNK_TIMEOUT, ranged, with_headers};
use crate::ctx::Ctx;
use crate::error::{Error, ErrorCode, Result};
use crate::http::Fetched;
use crate::model::{Downloaded, FileEntry, MediaKind, Page};
use crate::output::{note, progress};
use crate::paging::collect;
use crate::platform::PageReq;

/// Where a file's bytes are, and the request headers its host wants
/// (`User-Agent`, `Cookie`, `Referer` ...). The session's cookies are not
/// sent unless listed here; the headers go with every request of the file
/// and its redirects, except that `Cookie` stays on the link's site and
/// `Authorization` on its host. Requests are sequential byte ranges of one
/// connection at a time.
#[derive(Debug, Clone, Default)]
pub struct Link {
  pub url: String,
  pub headers: Vec<(String, String)>,
}

/// How many files one download of a folder fetches at most.
const MAX_FILES: usize = 5000;
/// How deep it follows folders inside folders.
const MAX_DEPTH: usize = 32;

/// Save `root` under `dir`: a file as `dir/<name>`, a folder as
/// `dir/<name>/…` with everything inside. `list` gives a folder's entries
/// a page at a time; `link(file, renew)` says where a file's bytes are, and
/// is asked again with `renew` when the host refuses a link mid-file.
pub async fn files(
  ctx: &Ctx,
  root: &FileEntry,
  dir: &Path,
  mut list: impl AsyncFnMut(&FileEntry, PageReq) -> Result<Page<FileEntry>>,
  mut link: impl AsyncFnMut(&FileEntry, bool) -> Result<Link>,
) -> Result<Vec<Downloaded>> {
  let plan = plan(root, dir, &mut list).await?;
  if plan.is_empty() {
    return Err(Error::not_found(format!("{} holds no files", root.label())));
  }
  let mut saved = Vec::new();
  for (file, out) in &plan {
    let bytes = save(ctx, file, out, async |renew| link(file, renew).await).await?;
    saved.push(Downloaded {
      kind: MediaKind::File,
      path: out.display().to_string(),
      bytes,
    });
  }
  Ok(saved)
}

/// Every file under `root`, and where it goes.
async fn plan(
  root: &FileEntry,
  dir: &Path,
  list: &mut impl AsyncFnMut(&FileEntry, PageReq) -> Result<Page<FileEntry>>,
) -> Result<Vec<(FileEntry, PathBuf)>> {
  let target = match root.name.trim_matches('/') {
    "" => dir.to_path_buf(),
    name => dir.join(safe_name(name)),
  };
  if !root.dir {
    return Ok(vec![(root.clone(), target)]);
  }
  let mut files = Vec::new();
  let mut folders = VecDeque::from([(root.clone(), target, 0)]);
  while let Some((folder, path, depth)) = folders.pop_front() {
    let page = collect(usize::MAX, None, async |req| list(&folder, req).await).await?;
    if page.has_more {
      return Err(Error::input(format!(
        "{} holds more entries than one listing reaches; download its folders one by one",
        folder.label()
      )));
    }
    for entry in page.items {
      let target = path.join(safe_name(&entry.name));
      if !entry.dir {
        files.push((entry, target));
      } else if depth < MAX_DEPTH {
        folders.push_back((entry, target, depth + 1));
      } else {
        return Err(Error::input(format!(
          "folders nest deeper than {MAX_DEPTH} levels in {}",
          root.label()
        )));
      }
    }
    if files.len() > MAX_FILES {
      return Err(Error::input(format!(
        "{} holds more than {MAX_FILES} files; download its folders one by one",
        root.label()
      )));
    }
  }
  Ok(files)
}

/// Fetch one file to `out` through `out.part`, continuing what an earlier
/// run left there; a file of the expected size already at `out` is kept.
async fn save(
  ctx: &Ctx,
  file: &FileEntry,
  out: &Path,
  mut link: impl AsyncFnMut(bool) -> Result<Link>,
) -> Result<u64> {
  if let Some(size) = file.size
    && local_size(out).await == Some(size)
  {
    note(&format!("{} is already there", out.display()));
    return Ok(size);
  }
  if let Some(parent) = out.parent() {
    tokio::fs::create_dir_all(parent).await?;
  }
  let part = part_path(out);
  let mut from = local_size(&part).await.unwrap_or(0);
  if file.size.is_some_and(|size| from > size) {
    from = 0;
  }
  let mut handle = tokio::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&part)
    .await?;
  if from == 0 {
    handle.set_len(0).await?;
  }
  let bar = progress(&file.name);
  bar.set_length(file.size.unwrap_or(0));
  bar.set_position(from);
  let result = match file.size {
    Some(size) if size == from => Ok(from),
    _ => fetch(ctx, from, &mut handle, &bar, &mut link).await,
  };
  bar.finish_and_clear();
  handle.flush().await?;
  let done = result.map_err(|e| resumable(e, &part))?;
  if let Some(size) = file.size
    && done != size
  {
    return Err(Error::network(format!(
      "got {done} of {size} bytes of {}; run download again to continue",
      file.name
    )));
  }
  tokio::fs::rename(&part, out).await?;
  Ok(done)
}

/// The bytes from `from` on, range by range; a range the host refuses
/// (an expired link) is asked again once with a renewed link.
async fn fetch(
  ctx: &Ctx,
  from: u64,
  file: &mut tokio::fs::File,
  bar: &ProgressBar,
  link: &mut impl AsyncFnMut(bool) -> Result<Link>,
) -> Result<u64> {
  if from > 0 {
    note(&format!("continuing at byte {from}"));
  }
  let mut current = link(false).await?;
  ranged(from, bar, async |a, b| {
    match chunk(ctx, &current, a, b, file, bar).await {
      Err(e) if expired(&e) => {
        current = link(true).await?;
        chunk(ctx, &current, a, b, file, bar).await
      }
      result => result,
    }
  })
  .await
}

/// Bytes `from..=to` appended to `file`; past the start they must come as
/// a range, or nothing is written.
async fn chunk(
  ctx: &Ctx,
  link: &Link,
  from: u64,
  to: u64,
  file: &mut tokio::fs::File,
  bar: &ProgressBar,
) -> Result<Fetched> {
  let req = with_headers(ctx.http.get(&link.url), &link.headers)
    .header("range", format!("bytes={from}-{to}"))
    .no_cookies()
    .no_throttle()
    .retries(2)
    .timeout(CHUNK_TIMEOUT);
  if from == 0 {
    req.append_to(file, |n| bar.inc(n)).await
  } else {
    req.append_range_to(file, |n| bar.inc(n)).await
  }
}

/// Whether the host refused the link itself (it expired, or wants a new
/// signature), as opposed to the connection failing.
fn expired(e: &Error) -> bool {
  match e.code {
    ErrorCode::PermissionDenied | ErrorCode::NotFound | ErrorCode::NotAuthenticated => true,
    ErrorCode::UpstreamError => e.message.starts_with("HTTP 4"),
    _ => false,
  }
}

/// Say where the partial file is, so the user knows a rerun continues.
fn resumable(e: Error, part: &Path) -> Error {
  if e.hint.is_some() {
    return e;
  }
  e.with_hint(format!(
    "the bytes so far are in {}; run download again to continue, or delete it to start over",
    part.display()
  ))
}

/// A server's file name made safe as one path component: no separators,
/// no `..`, nothing Windows refuses.
fn safe_name(name: &str) -> String {
  let cleaned: String = name
    .chars()
    .map(|c| {
      if c.is_control() || r#"/\:*?"<>|"#.contains(c) {
        '_'
      } else {
        c
      }
    })
    .collect();
  let cleaned = cleaned.trim().trim_end_matches(['.', ' ']);
  if cleaned.is_empty() {
    "_".to_owned()
  } else {
    cleaned.to_owned()
  }
}

fn part_path(out: &Path) -> PathBuf {
  let mut name = out.file_name().unwrap_or_default().to_os_string();
  name.push(".part");
  out.with_file_name(name)
}

async fn local_size(path: &Path) -> Option<u64> {
  let meta = tokio::fs::metadata(path).await.ok()?;
  meta.is_file().then_some(meta.len())
}
