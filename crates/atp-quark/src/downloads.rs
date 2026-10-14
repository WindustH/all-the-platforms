//! Where a file's bytes are. The link comes from `/file/download`, asked
//! for with the PC client's User-Agent (the drive answers browsers `23018`
//! for big files), and is fetched with the same User-Agent, the session
//! cookie and the drive as `Referer`. One connection, sequential ranges:
//! nothing here changes the account's download speed.

use atp_core::download::Link;
use atp_core::{Error, FileEntry, Page, PageReq, Result, Value, ValueExt, json};

use crate::api::{Api, CLIENT_UA, REFERER};
use crate::{files, tasks};

/// A folder's entries, for downloading the folder.
pub async fn children(api: &Api, folder: &FileEntry, req: &PageReq) -> Result<Page<FileEntry>> {
  refuse_share(folder)?;
  files::list(api, folder, None, req).await
}

pub async fn link(api: &Api, file: &FileEntry, renew: bool) -> Result<Link> {
  refuse_share(file)?;
  if file.dir {
    return Err(Error::input(format!("{} is a folder", file.label())));
  }
  if renew {
    tracing::debug!("asking for a new link to {}", file.id);
  }
  let body = json!({ "fids": [file.id] });
  let v = api
    .post_as("/file/download", &body, Some(CLIENT_UA))
    .await?;
  let url = match url_in(&v) {
    Some(url) => url,
    None => url_in(&tasks::settle(api, &v).await?)
      .ok_or_else(|| Error::upstream(format!("Quark gave no link to {}", file.label())))?,
  };
  Ok(Link {
    url,
    headers: vec![
      ("user-agent".into(), CLIENT_UA.into()),
      ("referer".into(), REFERER.into()),
      ("cookie".into(), api.ctx.http.cookie_header()),
    ],
  })
}

/// Files are not fetched out of someone's share.
fn refuse_share(entry: &FileEntry) -> Result<()> {
  if entry.share.is_none() {
    return Ok(());
  }
  Err(
    Error::input(format!("{} is in someone's share", entry.label()))
      .with_hint("save the share into your drive (`atp quark save URL`), then download from there"),
  )
}

/// The link in an answer: a list in `data`, or a finished task's.
fn url_in(v: &Value) -> Option<String> {
  [
    "data.0.download_url",
    "data.task_resp.data.0.download_url",
    "download_url",
    "task_resp.data.0.download_url",
  ]
  .iter()
  .find_map(|path| v.str(path))
}
