//! Uploads of your own audio files to your music cloud drive (云盘上传).
//!
//! The drive is asked first (`cloud/upload/check`): a file it holds
//! already by md5 needs no bytes (秒传). Otherwise the bytes go to NetEase's
//! object storage (NOS) in 4 MB parts, each part carrying the context the
//! one before it got back; a stopped upload continues with the next part.
//! The file's details (`upload/cloud/info/v2`) and its publication into the
//! drive (`cloud/pub/v2`) finish it.

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;

use atp_core::upload::{LocalFile, Parts, Resume};
use atp_core::{Data, Error, ErrorCode, FileEntry, Page, Result, ValueExt, json};
use serde::{Deserialize, Serialize};

use crate::api::Api;

const BUCKET: &str = "jd-musicrep-privatecloud-audio-public";
/// Where NOS says which upload hosts serve the bucket.
const LBS: &str = "https://wanproxy.127.net/lbs";
const PART_SIZE: u64 = 4 << 20;
/// The bitrate the web uploader declares for every file.
const BITRATE: &str = "999000";
/// How long an upload that stopped can be continued.
const RESUME_TTL: Duration = Duration::from_secs(6 * 3600);
/// Extensions the drive takes.
pub const AUDIO: &[&str] = &[
  "mp3", "flac", "ape", "wav", "m4a", "aac", "ogg", "opus", "wma", "aiff", "aif", "dsf", "dff",
];

/// Song details the drive shows for an upload.
#[derive(Debug, Default)]
pub struct Tags {
  pub artist: Option<String>,
  pub album: Option<String>,
}

/// One file's upload: what the parts and the finish need.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Session {
  md5: String,
  song_id: String,
  object: String,
  token: String,
  resource: String,
  /// The NOS host; empty when the drive holds the file already.
  host: String,
}

pub async fn upload(api: &Api, files: &[PathBuf], tags: &Tags) -> Result<Data> {
  api.require_login()?;
  for path in files {
    let ext = extension(path);
    if !AUDIO.contains(&ext.as_str()) {
      return Err(Error::input(format!(
        "{}: the cloud drive takes audio files ({})",
        path.display(),
        AUDIO.join(", ")
      )));
    }
  }
  let mut done = Vec::new();
  for path in files {
    done.push(one(api, path, tags).await?);
  }
  Ok(Data::Files(Page::last(done)))
}

async fn one(api: &Api, path: &std::path::Path, tags: &Tags) -> Result<FileEntry> {
  let local = LocalFile::open(path).await?;
  let parts = Parts::new(&api.ctx, &local, "ncm-cloud");
  let mut state = match parts.resume::<Session>(RESUME_TTL) {
    Some(state) => state,
    None => Resume::new(start(api, &local).await?),
  };
  let instant = state.session.host.is_empty();
  let result = async {
    if !instant {
      parts.keep(&state);
      send(api, &parts, &local, &mut state).await?;
    }
    finish(api, &local, &state.session, tags).await
  }
  .await;
  match result {
    Ok(mut entry) => {
      parts.forget();
      entry.extra.insert("instant".into(), json!(instant));
      Ok(entry)
    }
    Err(e) if matches!(e.code, ErrorCode::NetworkError | ErrorCode::RateLimited) => Err(e),
    Err(e) => {
      parts.forget();
      Err(e.with_hint("the upload cannot continue; run upload again to start over"))
    }
  }
}

/// Ask the drive about the file and get a place for its bytes.
async fn start(api: &Api, local: &LocalFile) -> Result<Session> {
  let md5 = local.hashes().await?.md5;
  let check = api
    .client("cloud/upload/check")
    .arg("bitrate", BITRATE)
    .arg("ext", "")
    .arg("length", local.size)
    .arg("md5", md5.as_str())
    .arg("songId", "0")
    .arg("version", 1)
    .write()
    .send()
    .await?;
  let song_id = check
    .str("songId")
    .ok_or_else(|| Error::upstream("cloud/upload/check answered without a songId"))?;
  let path = PathBuf::from(&local.name);
  // The object name the web uploader asks for: no spaces, no dots.
  let stem: String = title(&local.name)
    .chars()
    .filter(|c| !c.is_whitespace())
    .map(|c| if c == '.' { '_' } else { c })
    .collect();
  let token = api
    .web("nos/token/alloc")
    .arg("bucket", BUCKET)
    .arg("ext", extension(&path))
    .arg("filename", stem)
    .arg("local", false)
    .arg("nos_product", 3)
    .arg("type", "audio")
    .arg("md5", md5.as_str())
    .send()
    .await?;
  let field = |key: &str| {
    token
      .str(&format!("result.{key}"))
      .ok_or_else(|| Error::upstream(format!("nos/token/alloc answered without {key}")))
  };
  let host = match check.bool("needUpload") {
    Some(false) => String::new(),
    _ => host(api).await?,
  };
  Ok(Session {
    object: field("objectKey")?,
    token: field("token")?,
    resource: field("resourceId")?,
    md5,
    song_id,
    host,
  })
}

/// The first upload host NOS names for the bucket.
async fn host(api: &Api) -> Result<String> {
  let v = api
    .ctx
    .http
    .get(LBS)
    .query("version", "1.0")
    .query("bucketname", BUCKET)
    .no_cookies()
    .send()
    .await?
    .check()?
    .value()?;
  v.str("upload.0")
    .ok_or_else(|| Error::upstream("NOS named no upload host"))
}

/// Send the parts not sent yet; each one answers with the context of the
/// next.
async fn send(
  api: &Api,
  parts: &Parts<'_>,
  local: &LocalFile,
  state: &mut Resume<Session>,
) -> Result<()> {
  let session = state.session.clone();
  let context = RefCell::new(state.sent.last().cloned());
  let url = format!(
    "{}/{BUCKET}/{}",
    session.host.trim_end_matches('/'),
    session.object.replace('/', "%2F")
  );
  parts
    .send(state, PART_SIZE, async |part| {
      let offset = u64::from(part.number - 1) * PART_SIZE;
      let complete = offset + part.data.len() as u64 >= local.size;
      let mut req = api
        .ctx
        .http
        .post(&url)
        .no_cookies()
        .query("offset", offset)
        .query("complete", complete)
        .query("version", "1.0");
      if let Some(c) = context.borrow().as_deref() {
        req = req.query("context", c);
      }
      let v = req
        .header("x-nos-token", &session.token)
        .bytes(part.data.clone(), local.mime)
        .send()
        .await?
        .check()?
        .value()?;
      let next = v
        .str("context")
        .ok_or_else(|| Error::upstream("NOS answered a part without a context"))?;
      *context.borrow_mut() = Some(next.clone());
      Ok(next)
    })
    .await
}

/// Describe the file to the drive and publish it there.
async fn finish(api: &Api, local: &LocalFile, session: &Session, tags: &Tags) -> Result<FileEntry> {
  let info = api
    .client("upload/cloud/info/v2")
    .arg("md5", session.md5.as_str())
    .arg("songid", session.song_id.as_str())
    .arg("filename", local.name.as_str())
    .arg("song", title(&local.name))
    .arg("album", tags.album.as_deref().unwrap_or("未知专辑"))
    .arg("artist", tags.artist.as_deref().unwrap_or("未知艺术家"))
    .arg("bitrate", BITRATE)
    .arg("resourceId", session.resource.as_str())
    .write()
    .send()
    .await?;
  let id = info
    .str("songId")
    .unwrap_or_else(|| session.song_id.clone());
  api
    .client("cloud/pub/v2")
    .arg("songid", id.as_str())
    .write()
    .send()
    .await?;
  Ok(FileEntry {
    id,
    name: local.name.clone(),
    size: Some(local.size),
    md5: Some(session.md5.clone()),
    ..FileEntry::default()
  })
}

/// The file name without its extension.
fn title(name: &str) -> String {
  std::path::Path::new(name)
    .file_stem()
    .map_or_else(|| name.to_owned(), |s| s.to_string_lossy().into_owned())
}

fn extension(path: &std::path::Path) -> String {
  path
    .extension()
    .and_then(|e| e.to_str())
    .map(str::to_ascii_lowercase)
    .unwrap_or_default()
}
