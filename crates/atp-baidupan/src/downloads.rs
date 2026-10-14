//! Where a file's bytes are: downloads go only through the open platform.
//! `filemetas` with `dlink=1` gives the documented link (good for about 8
//! hours), fetched with the app's token and `User-Agent: pan.baidu.com`,
//! one connection and one range at a time; an expired link is asked for
//! again mid-file. Without the open-platform login there is no download:
//! atp signs nothing in the name of Baidu's own clients and changes
//! nothing about the account's speed.

use atp_core::download::Link;
use atp_core::{Error, FileEntry, Page, PageReq, Result, ValueExt};

use crate::api::Api;
use crate::files;
use crate::open::UA;

/// A folder's entries, for downloading the folder.
pub async fn children(api: &Api, folder: &FileEntry, req: &PageReq) -> Result<Page<FileEntry>> {
  refuse_share(folder)?;
  // Say so before walking a whole folder.
  api.open().require("downloading")?;
  let dir = folder.path.as_deref().unwrap_or(&folder.id);
  files::list(api, dir, None, req).await
}

pub async fn link(api: &Api, file: &FileEntry, renew: bool) -> Result<Link> {
  refuse_share(file)?;
  if file.dir {
    return Err(Error::input(format!("{} is a folder", file.label())));
  }
  let open = api.open();
  open.require("downloading")?;
  if renew {
    tracing::debug!("asking for a new link to {}", file.label());
  }
  let fs_id = match file.extra.get("fs_id").and_then(|v| v.as_str()) {
    Some(fs_id) => fs_id.to_owned(),
    None => fs_id_of(api, file).await?,
  };
  let item = files::meta(api, &fs_id, true).await?;
  let dlink = item
    .str("dlink")
    .ok_or_else(|| Error::upstream(format!("Baidu gave no link to {}", file.label())))?;
  let token = open.token().await?;
  let sep = if dlink.contains('?') { '&' } else { '?' };
  Ok(Link {
    url: format!("{dlink}{sep}access_token={token}"),
    headers: vec![("user-agent".into(), UA.into())],
  })
}

/// The file id of an entry known by its path only.
async fn fs_id_of(api: &Api, file: &FileEntry) -> Result<String> {
  let path = file.path.as_deref().unwrap_or(&file.id);
  let entry = files::at(api, path).await?;
  entry
    .extra
    .get("fs_id")
    .and_then(|v| v.as_str())
    .map(str::to_owned)
    .ok_or_else(|| Error::upstream(format!("Baidu listed {path} without a file id")))
}

/// Files are not fetched out of someone's share.
fn refuse_share(entry: &FileEntry) -> Result<()> {
  if entry.share.is_none() {
    return Ok(());
  }
  Err(
    Error::input(format!("{} is in someone's share", entry.label())).with_hint(
      "save the share into your drive (`atp baidupan save URL`), then download from there",
    ),
  )
}
