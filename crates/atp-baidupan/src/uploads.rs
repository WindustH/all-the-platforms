//! Uploads, through the open platform only (the documented way): the
//! drive is told the file's 4 MB block md5s, its md5 and the md5 of its
//! first 256 KB (`precreate`), which may finish it at once when the drive
//! holds the file already (秒传). Otherwise the blocks go up one by one to
//! the host `locateupload` names (`superfile2`) and the file is created
//! from them (`create`). An upload that stopped continues at its next
//! block when run again.
//!
//! Parts are 4 MB, the size every account may send, so one file goes up to
//! 4 GB (1024 parts); bigger files need the larger parts of members, which
//! atp does not send.

use std::path::{Path, PathBuf};
use std::time::Duration;

use atp_core::http::{Method, Part as Field};
use atp_core::upload::{BLOCK, Hashes, LocalFile, Part, Parts, Resume};
use atp_core::{Data, Error, ErrorCode, FileEntry, Page, Result, Value, ValueExt, json};
use serde::{Deserialize, Serialize};

use crate::api::{Api, Route, WEB_APP_ID, check};
use crate::{files, parse, refs};

/// How long an upload that stopped can be continued.
const RESUME_TTL: Duration = Duration::from_secs(12 * 3600);
const MAX_PARTS: u32 = 1024;
const UPLOAD_HOST: &str = "https://d.pcs.baidu.com";
const PART_TIMEOUT: Duration = Duration::from_secs(300);

/// The upload the drive set up for one file, kept between runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
  pub uploadid: String,
  pub path: String,
  /// `rtype`: 1 renames on a clash, 3 overwrites.
  pub rtype: u8,
  /// The upload host, with its scheme.
  pub host: String,
}

/// Upload local files into `to` (a folder of your drive; the root without
/// it). `overwrite` replaces files of the same name instead of renaming the
/// new one.
pub async fn upload(
  api: &Api,
  local: &[PathBuf],
  to: Option<&str>,
  overwrite: bool,
) -> Result<Data> {
  api.open().require("uploading")?;
  let dir = match to {
    Some(arg) => files::path_of(api, &api.ctx.post_ref(arg)?).await?,
    None => "/".to_owned(),
  };
  let rtype = if overwrite { 3 } else { 1 };
  let mut done = Vec::new();
  for path in local {
    done.push(one(api, path, &dir, rtype).await?);
  }
  Ok(Data::Files(Page::last(done)))
}

async fn one(api: &Api, path: &Path, dir: &str, rtype: u8) -> Result<FileEntry> {
  let local = LocalFile::open(path).await?;
  fits(&local)?;
  let target = refs::join(dir, &local.name);
  let parts = Parts::new(&api.ctx, &local, &target);
  if let Some(state) = parts.resume::<Session>(RESUME_TTL) {
    return send(api, &local, state).await;
  }
  let hashes = local.hashes().await?;
  let pre = precreate(api, &local, &target, &hashes, rtype).await?;
  if pre.i64("return_type") == Some(2) {
    let mut entry = entry(pre.at("info"), &local, &target);
    entry.extra.insert("instant".into(), json!(true));
    return Ok(entry);
  }
  let uploadid = pre
    .str("uploadid")
    .ok_or_else(|| Error::upstream("Baidu's precreate gave no uploadid"))?;
  let host = upload_host(api, &target, &uploadid).await;
  let state = Resume::new(Session {
    uploadid,
    path: target,
    rtype,
    host,
  });
  parts.keep(&state);
  send(api, &local, state).await
}

/// Files Baidu takes in 4 MB parts: not empty, at most 1024 parts.
fn fits(local: &LocalFile) -> Result<()> {
  if local.size == 0 {
    return Err(Error::input(format!(
      "{} is empty; Baidu takes no empty files",
      local.name
    )));
  }
  if local.parts(BLOCK) > MAX_PARTS {
    return Err(Error::input(format!(
      "{} is over 4 GB, more than one upload in 4 MB parts takes",
      local.name
    )));
  }
  Ok(())
}

/// Send the parts not sent yet, then create the file from them.
async fn send(api: &Api, local: &LocalFile, mut state: Resume<Session>) -> Result<FileEntry> {
  let session = state.session.clone();
  let parts = Parts::new(&api.ctx, local, &session.path);
  let token = api.open().token().await?;
  let result = async {
    parts
      .send(&mut state, BLOCK, async |part| {
        put(api, &session, &token, part).await
      })
      .await?;
    create(api, local, &session, &state.sent).await
  }
  .await;
  match result {
    Ok(v) => {
      parts.forget();
      Ok(entry(&v, local, &session.path))
    }
    Err(e) if matches!(e.code, ErrorCode::NetworkError | ErrorCode::RateLimited) => Err(e),
    Err(e) => {
      parts.forget();
      Err(e.with_hint("the upload cannot continue; run upload again to start over"))
    }
  }
}

async fn precreate(
  api: &Api,
  local: &LocalFile,
  target: &str,
  hashes: &Hashes,
  rtype: u8,
) -> Result<Value> {
  let secs = (local.modified_ms / 1000).to_string();
  let form = [
    ("path", target.to_owned()),
    ("size", local.size.to_string()),
    ("isdir", "0".into()),
    ("autoinit", "1".into()),
    ("rtype", rtype.to_string()),
    ("block_list", json!(hashes.blocks).to_string()),
    ("content-md5", hashes.md5.clone()),
    ("slice-md5", hashes.head_md5.clone()),
    ("local_ctime", secs.clone()),
    ("local_mtime", secs),
  ];
  let query = [("method", "precreate".to_owned())];
  api.open().post("/file", &query, &form).await
}

/// The host to send parts to, as `locateupload` names it; the default
/// host when it does not.
async fn upload_host(api: &Api, target: &str, uploadid: &str) -> String {
  let query = [
    ("method", "locateupload".to_owned()),
    ("appid", WEB_APP_ID.to_owned()),
    ("path", target.to_owned()),
    ("uploadid", uploadid.to_owned()),
    ("upload_version", "2.0".into()),
  ];
  let url = format!("{UPLOAD_HOST}/rest/2.0/pcs/file");
  match api.open().get(&url, &query).await {
    Ok(v) => v
      .list("servers")
      .iter()
      .filter_map(|s| s.str("server"))
      .find(|s| s.starts_with("https://"))
      .unwrap_or_else(|| UPLOAD_HOST.to_owned()),
    Err(e) => {
      tracing::debug!("locateupload failed ({}); using {UPLOAD_HOST}", e.message);
      UPLOAD_HOST.to_owned()
    }
  }
}

/// Send one part; the md5 the drive knows it by.
async fn put(api: &Api, session: &Session, token: &str, part: &Part) -> Result<String> {
  let url = format!(
    "{}/rest/2.0/pcs/superfile2",
    session.host.trim_end_matches('/')
  );
  let field = Field::file(
    "file",
    part.data.clone(),
    "blob",
    "application/octet-stream",
  );
  let resp = api
    .open()
    .request(Method::POST, &url, token)
    .query("method", "upload")
    .query("type", "tmpfile")
    .query("path", &session.path)
    .query("uploadid", &session.uploadid)
    .query("partseq", part.number - 1)
    .multipart(vec![field])
    .no_throttle()
    .retries(0)
    .timeout(PART_TIMEOUT)
    .send()
    .await?;
  let v = check(&resp, "superfile2", Route::Open)?;
  v.str("md5")
    .ok_or_else(|| Error::upstream(format!("Baidu took part {} without its md5", part.number)))
}

async fn create(api: &Api, local: &LocalFile, session: &Session, md5s: &[String]) -> Result<Value> {
  let secs = (local.modified_ms / 1000).to_string();
  let form = [
    ("path", session.path.clone()),
    ("size", local.size.to_string()),
    ("isdir", "0".into()),
    ("block_list", json!(md5s).to_string()),
    ("uploadid", session.uploadid.clone()),
    ("rtype", session.rtype.to_string()),
    ("local_ctime", secs.clone()),
    ("local_mtime", secs),
  ];
  let query = [("method", "create".to_owned())];
  api.open().post("/file", &query, &form).await
}

/// The uploaded file as the drive answered it, else as atp sent it.
fn entry(v: &Value, local: &LocalFile, target: &str) -> FileEntry {
  parse::file(v).unwrap_or_else(|| FileEntry {
    id: target.to_owned(),
    name: local.name.clone(),
    path: Some(target.to_owned()),
    size: Some(local.size),
    ..FileEntry::default()
  })
}
