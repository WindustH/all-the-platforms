//! Baidu payloads → core models: files and folders (both routes and share
//! listings answer alike), trashed files, own shares, offline tasks, the
//! account and its storage.

use atp_core::text::{fmt_size, from_secs};
use atp_core::{FileEntry, ShareLink, User, Value, ValueExt, json};

use crate::refs;

/// A file or folder: its `id` is its path, its file id is `fs_id` in `extra`.
pub fn file(v: &Value) -> Option<FileEntry> {
  let path = v.str("path").filter(|p| p.starts_with('/'))?;
  let name = v
    .first_str(&["server_filename", "filename"])
    .unwrap_or_else(|| refs::split(&path).1);
  let dir = v.i64("isdir") == Some(1);
  let mut entry = FileEntry {
    id: path.clone(),
    name,
    path: Some(path),
    dir,
    size: v.u64("size").filter(|_| !dir),
    updated_at: time(v, &["server_mtime", "mtime", "server_ctime"]),
    md5: v.str("md5").filter(|_| !dir),
    raw: Some(v.clone()),
    ..FileEntry::default()
  };
  if let Some(fs_id) = v.str("fs_id") {
    entry.extra.insert("fs_id".into(), json!(fs_id));
  }
  if let Some(kind) = v.i64("category").and_then(category).filter(|_| !dir) {
    entry.extra.insert("type".into(), json!(kind));
  }
  Some(entry)
}

/// A file in the recycle bin: its `id` is the file id that `restore` and
/// `purge` take, its `path` where it was.
pub fn trashed(v: &Value) -> Option<FileEntry> {
  let fs_id = v.str("fs_id")?;
  let mut entry = file(v)?;
  entry.extra.remove("fs_id");
  entry.id = fs_id;
  if let Some(days) = v.i64("leftTime") {
    entry.extra.insert("days_left".into(), json!(days));
  }
  Some(entry)
}

/// One of the account's own shares: `id` is the share id `unshare` takes.
pub fn share(v: &Value) -> Option<FileEntry> {
  let id = v.first_str(&["shareId", "shareid"])?;
  let files = v.list("fsIds").len().max(1);
  let path = v.str("typicalPath").filter(|p| p.starts_with('/'));
  let expired = v.i64("expiredType") == Some(-1);
  let mut entry = FileEntry {
    name: path
      .as_deref()
      .map(|p| refs::split(p).1)
      .unwrap_or_else(|| id.clone()),
    path,
    id,
    dir: files > 1 || v.i64("typicalCategory") == Some(-1),
    updated_at: time(v, &["ctime"]),
    share: Some(ShareLink {
      url: v.first_str(&["shortlink", "shorturl"]).unwrap_or_default(),
      code: None,
      expires_at: time(v, &["lastExpireTime"]).filter(|_| !expired),
    }),
    raw: Some(v.clone()),
    ..FileEntry::default()
  };
  entry.extra.insert("files".into(), json!(files));
  for (key, field) in [("views", "vCnt"), ("saves", "tCnt"), ("downloads", "dCnt")] {
    if let Some(n) = v.u64(field) {
      entry.extra.insert(key.into(), json!(n));
    }
  }
  if expired {
    entry.extra.insert("status".into(), json!("expired"));
  } else if v.i64("status").is_some_and(|s| s != 0) {
    entry.extra.insert("status".into(), json!("blocked"));
  }
  Some(entry)
}

/// An offline download: `id` is the task id `offline cancel` takes, `path`
/// the folder it saves into.
pub fn task(v: &Value) -> Option<FileEntry> {
  let id = v.str("task_id")?;
  let size = v.u64("file_size").filter(|s| *s > 0);
  let mut entry = FileEntry {
    name: v
      .first_str(&["task_name", "source_url"])
      .unwrap_or_else(|| id.clone()),
    path: v.str("save_path"),
    size,
    updated_at: time(v, &["finish_time", "start_time", "create_time"]),
    raw: Some(v.clone()),
    id,
    ..FileEntry::default()
  };
  let status = v.i64("status").unwrap_or(-1);
  entry
    .extra
    .insert("status".into(), json!(task_status(status)));
  if let (Some(done), Some(size)) = (v.u64("finished_size"), size) {
    let percent = (done.min(size) * 100 / size.max(1)).to_string() + "%";
    entry.extra.insert("progress".into(), json!(percent));
  }
  if let Some(url) = v.str("source_url") {
    entry.extra.insert("source".into(), json!(url));
  }
  Some(entry)
}

fn task_status(status: i64) -> &'static str {
  match status {
    0 => "done",
    1 => "running",
    2 => "system error",
    3 => "source not found",
    4 => "timed out",
    5 => "failed",
    6 => "drive full",
    7 => "cancelled",
    _ => "unknown",
  }
}

/// The account behind the web login (`/api/loginStatus`) and its storage.
pub fn web_user(info: &Value, quota: &Value) -> User {
  let vip = info
    .i64("vip_type")
    .or_else(|| (info.i64("is_svip") == Some(1)).then_some(2))
    .or_else(|| (info.i64("is_vip") == Some(1)).then_some(1));
  let mut user = user(
    info.first_str(&["username", "uname", "netdisk_name"]),
    info.str("uk"),
    info.first_str(&["photo_url", "avatar_url"]),
    vip,
  );
  if let (Some(used), Some(total)) = (quota.u64("used"), quota.u64("total")) {
    let storage = format!("{} of {}", fmt_size(used), fmt_size(total));
    user.extra.insert("storage".into(), json!(storage));
  }
  user.raw = Some(json!({ "login_info": info, "quota": quota }));
  user
}

/// The account behind the open-platform token (`uinfo`).
pub fn open_user(v: &Value) -> User {
  let mut user = user(
    v.first_str(&["netdisk_name", "baidu_name"]),
    v.str("uk"),
    v.str("avatar_url"),
    v.i64("vip_type"),
  );
  user.raw = Some(v.clone());
  user
}

fn user(
  name: Option<String>,
  uk: Option<String>,
  avatar: Option<String>,
  vip: Option<i64>,
) -> User {
  let name = name.unwrap_or_else(|| "Baidu Netdisk user".into());
  let mut user = User {
    id: uk.unwrap_or_else(|| name.clone()),
    name,
    avatar,
    ..User::default()
  };
  if let Some(vip) = vip {
    user.extra.insert("member".into(), json!(member(vip)));
  }
  user
}

/// Baidu's member types in words.
pub fn member(vip: i64) -> &'static str {
  match vip {
    1 => "VIP",
    2 => "SVIP",
    _ => "free",
  }
}

/// Storage used and left (`/api/quota`, either route).
pub fn quota(v: &Value) -> Value {
  let total = v.u64("total").unwrap_or(0);
  let used = v.u64("used").unwrap_or(0);
  let free = total.saturating_sub(used);
  json!({
    "total_bytes": total,
    "used_bytes": used,
    "free_bytes": free,
    "total": fmt_size(total),
    "used": fmt_size(used),
    "free": fmt_size(free),
  })
}

fn category(code: i64) -> Option<&'static str> {
  Some(match code {
    1 => "video",
    2 => "audio",
    3 => "image",
    4 => "document",
    5 => "app",
    7 => "torrent",
    _ => return None,
  })
}

/// The first of `keys` holding a time in Unix seconds (0 is none).
fn time(v: &Value, keys: &[&str]) -> Option<jiff::Timestamp> {
  keys
    .iter()
    .find_map(|k| v.i64(k).filter(|s| *s > 0))
    .and_then(from_secs)
}
