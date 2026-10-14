//! Your music cloud drive (音乐云盘): the files you uploaded yourself,
//! listed, deleted and downloaded. Downloads take only ids your own drive
//! lists, through the drive's own download link.

use std::path::Path;

use atp_core::cli::confirm;
use atp_core::download::{self, Link};
use atp_core::{Action, Data, Error, FileEntry, Page, PageReq, Result, ValueExt, json};

use crate::api::Api;
use crate::{parse, refs};

/// One page of the drive, newest first; the cursor is an offset.
pub async fn list(api: &Api, page: &PageReq) -> Result<Page<FileEntry>> {
  api.require_login()?;
  let offset = page.number_or(0);
  let v = api
    .web("v1/cloud/get")
    .arg("limit", page.size_within(100) as u64)
    .arg("offset", offset)
    .send()
    .await?;
  let list = v.list("data");
  let files = list.iter().filter_map(parse::cloud_file).collect();
  let end = offset + list.len() as u64;
  let more = v.bool("hasMore") == Some(true) && !list.is_empty();
  Ok(Page::new(files, more.then(|| end.to_string())))
}

/// Space used and allowed, in bytes.
pub async fn quota(api: &Api) -> Result<Data> {
  api.require_login()?;
  let v = api
    .web("v1/cloud/get")
    .arg("limit", 1)
    .arg("offset", 0)
    .send()
    .await?;
  Ok(Data::Value(json!({
    "files": v.u64("count"),
    "used": v.u64("size"),
    "total": v.u64("maxSize"),
  })))
}

/// One file of your drive; an id the drive does not list is `not_found`.
pub async fn file(api: &Api, arg: &str) -> Result<FileEntry> {
  api.require_login()?;
  let id = refs::number(arg, "cloud file")?;
  let v = api
    .web("v1/cloud/get/byids")
    .arg("songIds", json!([id]))
    .send()
    .await?;
  v.list("data")
    .iter()
    .find_map(parse::cloud_file)
    .ok_or_else(|| Error::not_found(format!("{id} is not in your cloud drive")))
}

/// Delete files of your drive, after a confirmation.
pub async fn delete(api: &Api, args: &[String], yes: bool) -> Result<Action> {
  api.require_login()?;
  let ids = args
    .iter()
    .map(|a| refs::number(a, "cloud file"))
    .collect::<Result<Vec<u64>>>()?;
  confirm(
    yes,
    &format!("delete {} file(s) from your cloud drive", ids.len()),
  )?;
  api
    .web("cloud/del")
    .arg("songIds", json!(ids))
    .write()
    .send()
    .await?;
  Ok(Action::done(
    "delete",
    ids.iter().map(u64::to_string).collect::<Vec<_>>().join(","),
  ))
}

/// Save files of your drive under `dir`, by the names you uploaded them as.
pub async fn download(api: &Api, args: &[String], dir: &Path) -> Result<Data> {
  let mut saved = Vec::new();
  for arg in args {
    let entry = file(api, arg).await?;
    let done = download::files(
      &api.ctx,
      &entry,
      dir,
      async |_, _| Ok(Page::last(Vec::new())),
      async |f, _| link(api, f).await,
    )
    .await?;
    saved.extend(done);
  }
  Ok(Data::Downloads(saved))
}

/// The drive's download link of one of your files.
async fn link(api: &Api, file: &FileEntry) -> Result<Link> {
  let v = api
    .client("cloud/dowonload")
    .arg("songId", file.id.as_str())
    .send()
    .await?;
  let url = v
    .first_str(&["data.url", "url", "data.downloadUrl"])
    .filter(|u| u.starts_with("http"))
    .ok_or_else(|| Error::upstream(format!("no download link for {}", file.name)))?;
  Ok(Link {
    url,
    headers: Vec::new(),
  })
}
