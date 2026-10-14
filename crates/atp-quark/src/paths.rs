//! Paths to file ids. Quark knows files and folders by id; a path is found
//! by listing folder after folder from the deepest one already known. The
//! folders found are cached for a day (`paths` in the private cache); moves,
//! renames and deletes drop that cache, and a cached folder that is gone
//! sends the walk back to the root.

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::{Error, ErrorCode, FileEntry, PageReq, Result};

use crate::api::Api;
use crate::{files, refs};

const CACHE: &str = "paths";
const TTL: Duration = Duration::from_secs(24 * 3600);
/// Pages of one folder searched for a name.
const MAX_PAGES: usize = 50;

type Known = BTreeMap<String, String>;

/// The drive's root folder.
pub fn root() -> FileEntry {
  FileEntry {
    id: "0".into(),
    path: Some("/".into()),
    dir: true,
    ..FileEntry::default()
  }
}

/// The file or folder at `path`.
pub async fn resolve(api: &Api, path: &str) -> Result<FileEntry> {
  let path = refs::path(path);
  if path == "/" {
    return Ok(root());
  }
  let known = cached(api);
  if let Some(fid) = known.get(&path) {
    return Ok(folder(fid, &path));
  }
  match walk(api, &path, known).await {
    Err(e) if e.code == ErrorCode::NotFound && !cached(api).is_empty() => {
      forget(api);
      walk(api, &path, Known::new()).await
    }
    result => result,
  }
}

/// A folder at `path`, which must be one.
pub async fn folder_at(api: &Api, path: &str) -> Result<FileEntry> {
  let entry = resolve(api, path).await?;
  if !entry.dir {
    return Err(Error::input(format!(
      "{} is a file, not a folder",
      entry.label()
    )));
  }
  Ok(entry)
}

/// Keep the folders of a listing whose paths are known.
pub fn remember(api: &Api, entries: &[FileEntry]) {
  let mut known = cached(api);
  let before = known.len();
  for entry in entries.iter().filter(|e| e.dir) {
    if let Some(path) = &entry.path {
      known.insert(path.clone(), entry.id.clone());
    }
  }
  if known.len() != before {
    api.ctx.store.cache_put(CACHE, &known);
  }
}

/// Drop the cache after the drive changed shape.
pub fn forget(api: &Api) {
  api.ctx.store.cache_remove(CACHE);
}

/// From the deepest known folder down to `path`, one listing per level.
async fn walk(api: &Api, path: &str, mut known: Known) -> Result<FileEntry> {
  let names: Vec<&str> = path.split('/').filter(|n| !n.is_empty()).collect();
  let (mut current, start) = deepest(&known, &names);
  for (i, name) in names.iter().enumerate().skip(start) {
    let Some(entry) = child(api, &current, name).await? else {
      return Err(Error::not_found(format!(
        "no {} in your drive",
        refs::join(current.path.as_deref().unwrap_or("/"), name)
      )));
    };
    if !entry.dir && i + 1 < names.len() {
      return Err(Error::input(format!(
        "{} is a file, not a folder",
        entry.label()
      )));
    }
    if entry.dir
      && let Some(p) = &entry.path
    {
      known.insert(p.clone(), entry.id.clone());
    }
    current = entry;
  }
  api.ctx.store.cache_put(CACHE, &known);
  Ok(current)
}

/// The deepest cached folder above the last name, and the index of the
/// first name below it.
fn deepest(known: &Known, names: &[&str]) -> (FileEntry, usize) {
  for depth in (1..names.len()).rev() {
    let prefix = format!("/{}", names[..depth].join("/"));
    if let Some(fid) = known.get(&prefix) {
      return (folder(fid, &prefix), depth);
    }
  }
  (root(), 0)
}

/// The entry called `name` in `parent`, page by page.
async fn child(api: &Api, parent: &FileEntry, name: &str) -> Result<Option<FileEntry>> {
  let mut cursor = None;
  for _ in 0..MAX_PAGES {
    let req = PageReq { cursor, size: 100 };
    let page = files::list(api, parent, None, &req).await?;
    if let Some(found) = page.items.into_iter().find(|e| e.name == name) {
      return Ok(Some(found));
    }
    match page.next_cursor {
      Some(next) if page.has_more => cursor = Some(next),
      _ => return Ok(None),
    }
  }
  Ok(None)
}

fn folder(fid: &str, path: &str) -> FileEntry {
  let (_, name) = refs::split(path);
  FileEntry {
    id: fid.to_owned(),
    name,
    path: Some(path.to_owned()),
    dir: true,
    ..FileEntry::default()
  }
}

fn cached(api: &Api) -> Known {
  api.ctx.store.cache_get(CACHE, TTL).unwrap_or_default()
}
