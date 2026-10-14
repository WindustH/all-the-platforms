//! Quark payloads → core models: files and folders (`file/sort`, `search`,
//! `info`, share pages), trashed files, own shares, the account.

use atp_core::text::{fmt_size, from_millis};
use atp_core::{FileEntry, ShareLink, User, Value, ValueExt, json};

use crate::refs;

/// A file or folder; `parent` is the path of the folder it was listed in.
pub fn file(v: &Value, parent: Option<&str>) -> Option<FileEntry> {
  let id = v.str("fid").filter(|f| !f.is_empty())?;
  let name = unescape(&v.str("file_name").unwrap_or_default());
  let dir = folder(v);
  let mut entry = FileEntry {
    path: parent
      .map(|p| refs::join(p, &name))
      .or_else(|| full_path(v)),
    size: v.u64("size").filter(|_| !dir),
    updated_at: time(v, &["updated_at", "l_updated_at", "created_at"]),
    md5: v.str("md5").filter(|m| !m.is_empty()),
    id,
    name,
    dir,
    raw: Some(v.clone()),
    ..FileEntry::default()
  };
  if let Some(items) = v.u64("include_items").filter(|_| dir) {
    entry.extra.insert("items".into(), json!(items));
  }
  if let Some(mime) = v.str("format_type").filter(|m| !m.is_empty() && !dir) {
    entry.extra.insert("type".into(), json!(mime));
  }
  Some(entry)
}

/// A file in the recycle bin: its `id` is the record that `restore` and
/// `purge` take.
pub fn trashed(v: &Value) -> Option<FileEntry> {
  let record = v.str("record_id").filter(|r| !r.is_empty())?;
  let mut entry = file(v, None).unwrap_or_else(|| FileEntry {
    name: unescape(&v.str("file_name").unwrap_or_default()),
    dir: folder(v),
    ..FileEntry::default()
  });
  if let Some(fid) = v.str("fid") {
    entry.extra.insert("fid".into(), json!(fid));
  }
  if let Some(from) = v.str("pdir_file_name").filter(|f| !f.is_empty()) {
    entry.extra.insert("from".into(), json!(unescape(&from)));
  }
  if let Some(at) = time(v, &["move_recycle_at", "deleted_at"]) {
    entry.extra.insert("deleted_at".into(), json!(at));
  }
  entry.id = record;
  entry.raw = Some(v.clone());
  Some(entry)
}

/// One of the account's own shares: `id` is the share id `unshare` takes.
pub fn share(v: &Value) -> Option<FileEntry> {
  let id = v.str("share_id").filter(|s| !s.is_empty())?;
  let files = v.u64("file_num").unwrap_or(1);
  let mut entry = FileEntry {
    id,
    name: unescape(
      &v.first_str(&["title", "first_file.file_name"])
        .unwrap_or_default(),
    ),
    dir: v.bool("first_file.dir").unwrap_or(files > 1),
    updated_at: time(v, &["created_at", "updated_at"]),
    share: Some(link(v)),
    raw: Some(v.clone()),
    ..FileEntry::default()
  };
  entry.extra.insert("files".into(), json!(files));
  for (key, field) in [
    ("views", "click_pv"),
    ("saves", "save_pv"),
    ("downloads", "download_pv"),
  ] {
    if let Some(n) = v.u64(field) {
      entry.extra.insert(key.into(), json!(n));
    }
  }
  if let Some(status) = v.i64("status").filter(|s| *s != 1) {
    entry.extra.insert("status".into(), json!(status));
  }
  Some(entry)
}

/// The link, code and end of a share; one that never ends (`expired_type`
/// 1) says 2099, which is left out.
pub fn link(v: &Value) -> ShareLink {
  let ends = v.i64("expired_type") != Some(1);
  ShareLink {
    url: v.str("share_url").unwrap_or_default(),
    code: v.str("passcode").filter(|c| !c.is_empty()),
    expires_at: time(v, &["expired_at"]).filter(|_| ends),
  }
}

/// The account from `account/info` and `member`.
pub fn user(account: &Value, member: &Value) -> User {
  let name = account
    .first_str(&["nickname", "nick_name"])
    .unwrap_or_else(|| "Quark user".into());
  let mut user = User {
    id: account
      .first_str(&["uid", "user_id"])
      .unwrap_or_else(|| name.clone()),
    avatar: account.first_str(&["avatarUri", "avatar_uri", "avatar"]),
    name,
    raw: Some(json!({ "account": account, "member": member })),
    ..User::default()
  };
  if let Some(kind) = member.str("member_type") {
    user
      .extra
      .insert("member".into(), json!(member_label(&kind)));
  }
  if let (Some(used), Some(total)) = (member.u64("use_capacity"), member.u64("total_capacity")) {
    let storage = format!("{} of {}", fmt_size(used), fmt_size(total));
    user.extra.insert("storage".into(), json!(storage));
  }
  user
}

/// Quark's member types in words.
pub fn member_label(kind: &str) -> String {
  match kind {
    "NORMAL" => "free".into(),
    "EXP_SVIP" => "88VIP (SVIP)".into(),
    "SUPER_VIP" => "SVIP".into(),
    "Z_VIP" => "Z VIP".into(),
    "VIP" => "VIP".into(),
    other => other.to_owned(),
  }
}

fn folder(v: &Value) -> bool {
  v.bool("dir")
    .or_else(|| v.bool("file").map(|f| !f))
    .unwrap_or_else(|| v.i64("file_type") == Some(0))
}

fn full_path(v: &Value) -> Option<String> {
  v.first_str(&["full_path", "path"])
    .filter(|p| p.starts_with('/'))
    .map(|p| unescape(&p))
}

/// The first of `keys` holding a time in milliseconds (0 is none).
pub fn time(v: &Value, keys: &[&str]) -> Option<jiff::Timestamp> {
  keys
    .iter()
    .find_map(|k| v.i64(k).filter(|ms| *ms > 0))
    .and_then(from_millis)
}

/// File names come HTML-escaped.
pub fn unescape(s: &str) -> String {
  if !s.contains('&') {
    return s.to_owned();
  }
  s.replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&#x27;", "'")
    .replace("&amp;", "&")
}
