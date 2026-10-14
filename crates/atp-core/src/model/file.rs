//! Files and folders of a drive (Quark, Baidu Netdisk ...), and their share
//! links.
//!
//! A drive lists [`FileEntry`]s where other platforms list posts: `ls`,
//! `search`, `read`, the trash and share listings all print them, and
//! `#N` after such a list refers to the N-th entry's `id`.

use jiff::Timestamp;
use serde::Serialize;
use serde_json::Value;

use super::*;

/// A share link: on a share of your own, or on the files inside someone's share.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ShareLink {
  pub url: String,
  /// The extraction code (提取码) the link asks for.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub code: Option<String>,
  /// When the link stops working; none for links that never expire.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub expires_at: Option<Timestamp>,
}

/// One file or folder of a drive, or one share of it.
///
/// - `id`: what the drive's commands take back (file id, share id, trash record id);
/// - `path`: the full path from the drive's root, when the listing tells it;
/// - `md5`: the hash the drive reports, as it reports it (Baidu's is not the
///   content md5 of files uploaded in several blocks); other hashes go in `extra`;
/// - `share`: set on shares, and on the files of someone's share.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FileEntry {
  pub id: String,
  pub name: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,
  /// A folder, not a file.
  pub dir: bool,
  /// Bytes of a file.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub size: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub updated_at: Option<Timestamp>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub md5: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub share: Option<ShareLink>,
  #[serde(skip_serializing_if = "Extra::is_empty")]
  pub extra: Extra,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub raw: Option<Value>,
}

impl FileEntry {
  /// What other commands should receive to refer back to this entry.
  pub fn reference(&self) -> &str {
    &self.id
  }

  /// `path` when known, else `name`; folders end in `/`.
  pub fn label(&self) -> String {
    let base = self.path.as_deref().unwrap_or(&self.name);
    if self.dir && !base.ends_with('/') {
      format!("{base}/")
    } else {
      base.to_owned()
    }
  }
}
