//! Uploads. The drive is asked first (`upload/pre`), then told the file's
//! md5 and sha1 (`update/hash`): a file it holds already is done at once
//! (秒传). Anything else goes to Aliyun OSS in parts ([`crate::oss`]), one
//! at a time, and is committed and finished; an upload that stopped
//! continues with the part after the last one sent.

use std::path::PathBuf;
use std::time::Duration;

use atp_core::upload::{LocalFile, Parts, Resume};
use atp_core::{Data, Error, ErrorCode, FileEntry, Page, Result, Value, ValueExt, json};
use serde::{Deserialize, Serialize};

use crate::api::Api;
use crate::{files, oss, paths, refs};

/// How long an upload that stopped can be continued.
const RESUME_TTL: Duration = Duration::from_secs(12 * 3600);

/// The OSS upload the drive set up for one file: what each part, the
/// commit and the finish need, kept between runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
  pub task_id: String,
  pub upload_id: String,
  pub obj_key: String,
  pub bucket: String,
  /// The OSS host, without the bucket.
  pub host: String,
  pub auth_info: String,
  pub callback: Value,
  pub part_size: u64,
  pub mime: String,
  pub fid: String,
}

/// Upload local files into `to` (a folder of your drive; the root without it).
pub async fn upload(api: &Api, local: &[PathBuf], to: Option<&str>) -> Result<Data> {
  let folder = match to {
    Some(arg) => files::folder(api, arg).await?,
    None => paths::root(),
  };
  let mut done = Vec::new();
  for path in local {
    done.push(one(api, path, &folder).await?);
  }
  Ok(Data::Files(Page::last(done)))
}

async fn one(api: &Api, path: &std::path::Path, folder: &FileEntry) -> Result<FileEntry> {
  let local = LocalFile::open(path).await?;
  let parts = Parts::new(&api.ctx, &local, &folder.id);
  if let Some(state) = parts.resume::<Session>(RESUME_TTL) {
    return send(api, &local, folder, state).await;
  }
  let hashes = local.hashes().await?;
  let pre = pre(api, &local, folder).await?;
  let mut entry = entry(&local, folder, pre.str("data.fid"));
  entry.md5 = Some(hashes.md5.clone());
  if pre.bool("data.finish") == Some(true) {
    return Ok(instant(entry));
  }
  let task_id = pre.str("data.task_id").unwrap_or_default();
  let body = json!({ "md5": hashes.md5, "sha1": hashes.sha1, "task_id": task_id });
  let hashed = api.post("/file/update/hash", &body).await?;
  if hashed.bool("data.finish") == Some(true) {
    return Ok(instant(entry));
  }
  let state = Resume::new(session(&pre, &local)?);
  parts.keep(&state);
  send(api, &local, folder, state).await
}

/// Send the parts not sent yet, commit and finish. Parts state where
/// SHA-1 stands when their size allows it.
async fn send(
  api: &Api,
  local: &LocalFile,
  folder: &FileEntry,
  mut state: Resume<Session>,
) -> Result<FileEntry> {
  let session = state.session.clone();
  let mut parts = Parts::new(&api.ctx, local, &folder.id);
  if session.part_size.is_multiple_of(64) {
    parts = parts.with_sha1();
  }
  let result = async {
    parts
      .send(&mut state, session.part_size, async |part| {
        oss::put(api, &session, part).await
      })
      .await?;
    oss::commit(api, &session, &state.sent).await?;
    let body = json!({ "obj_key": session.obj_key, "task_id": session.task_id });
    api.post("/file/upload/finish", &body).await
  }
  .await;
  match result {
    Ok(v) => {
      parts.forget();
      let fid = v.str("data.fid").or(Some(session.fid.clone()));
      Ok(entry(local, folder, fid))
    }
    Err(e) if matches!(e.code, ErrorCode::NetworkError | ErrorCode::RateLimited) => Err(e),
    Err(e) => {
      parts.forget();
      Err(e.with_hint("the upload cannot continue; run upload again to start over"))
    }
  }
}

async fn pre(api: &Api, local: &LocalFile, folder: &FileEntry) -> Result<Value> {
  let body = json!({
    "ccp_hash_update": true,
    "parallel_upload": false,
    "pdir_fid": folder.id,
    "file_name": local.name,
    "size": local.size,
    "format_type": local.mime,
    "dir_name": "",
    "l_created_at": local.modified_ms,
    "l_updated_at": local.modified_ms,
  });
  api.post("/file/upload/pre", &body).await
}

/// The OSS upload `upload/pre` set up.
fn session(pre: &Value, local: &LocalFile) -> Result<Session> {
  let data = pre.at("data");
  let field = |key: &str| {
    data
      .str(key)
      .ok_or_else(|| Error::upstream(format!("Quark's upload/pre gave no {key}")))
  };
  let upload_url = field("upload_url")?;
  let host = upload_url
    .trim_start_matches("https://")
    .trim_start_matches("http://")
    .trim_end_matches('/')
    .to_owned();
  Ok(Session {
    task_id: field("task_id")?,
    upload_id: field("upload_id")?,
    obj_key: field("obj_key")?,
    bucket: field("bucket")?,
    host,
    auth_info: field("auth_info")?,
    callback: data.at("callback").clone(),
    part_size: pre
      .u64("metadata.part_size")
      .filter(|s| *s > 0)
      .unwrap_or(4 << 20),
    mime: local.mime.to_owned(),
    fid: data.str("fid").unwrap_or_default(),
  })
}

fn entry(local: &LocalFile, folder: &FileEntry, fid: Option<String>) -> FileEntry {
  FileEntry {
    id: fid.unwrap_or_default(),
    path: folder.path.as_deref().map(|p| refs::join(p, &local.name)),
    name: local.name.clone(),
    size: Some(local.size),
    ..FileEntry::default()
  }
}

/// The drive had the file already.
fn instant(mut entry: FileEntry) -> FileEntry {
  entry.extra.insert("instant".into(), json!(true));
  entry
}
