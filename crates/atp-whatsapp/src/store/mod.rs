//! The linked device's keys and protocol state, kept in one private file.
//!
//! whatsapp-rust stores sessions through its `Backend` trait; its SQLite
//! backend cannot be linked next to the browser-cookie importer (both bundle
//! `sqlite3`), so this is its in-memory backend (`wacore::store::in_memory`,
//! MIT, by João Lucas) with a serializable state: every write marks it dirty
//! and [`FileBackend::flush`] writes it out (mode 0600) at the end of a
//! command and periodically while connected.

mod app_sync;
mod device;
mod protocol;
mod secrets;
mod signal;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use serde::{Deserialize, Serialize};
use whatsapp_rust::wacore::appstate::hash::HashState;
use whatsapp_rust::wacore::store::Device;
use whatsapp_rust::wacore::store::error::StoreError;
use whatsapp_rust::wacore::store::traits::*;

/// Bound on retained sent messages (kept for retries), as upstream.
const MAX_SENT_MESSAGES: usize = 4096;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct State {
  // Signal
  identities: HashMap<String, [u8; 32]>,
  sessions: HashMap<String, Vec<u8>>,
  prekeys: HashMap<u32, Vec<u8>>,
  signed_prekeys: HashMap<u32, Vec<u8>>,
  sender_keys: HashMap<String, Vec<u8>>,
  // App state sync; byte-keyed maps are lists (JSON keys are strings).
  sync_keys: Vec<(Vec<u8>, AppStateSyncKey)>,
  latest_sync_key_id: Option<Vec<u8>>,
  versions: HashMap<String, HashState>,
  mutation_macs: Vec<(String, Vec<u8>, Vec<u8>)>,
  // Protocol
  sender_key_devices: HashMap<String, HashMap<String, bool>>,
  lid_mappings: HashMap<String, LidPnMappingEntry>,
  pn_to_lid: HashMap<String, String>,
  base_keys: Vec<(String, String, Vec<u8>)>,
  device_lists: HashMap<String, DeviceListRecord>,
  group_metadata: HashMap<String, Vec<u8>>,
  tc_tokens: HashMap<String, TcTokenEntry>,
  /// `(chat, message id, payload, stored at)`.
  sent_messages: Vec<(String, String, Vec<u8>, i64)>,
  /// `(chat, sender, id, message, stored at)`.
  pending_inbound: Vec<(String, String, String, Vec<u8>, i64)>,
  /// `(chat, sender, id, secret, expires at, message ts)`.
  msg_secrets: Vec<(String, String, String, Vec<u8>, i64, i64)>,
  // Device
  device: Option<Device>,
}

pub struct FileBackend {
  path: PathBuf,
  state: tokio::sync::Mutex<State>,
  next_device_id: AtomicI32,
  dirty: AtomicBool,
}

impl FileBackend {
  /// The saved state at `path`, or an empty one.
  pub fn open(path: &Path) -> std::io::Result<Self> {
    let state = match std::fs::read(path) {
      Ok(bytes) => serde_json::from_slice(&bytes).map_err(std::io::Error::other)?,
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
      Err(e) => return Err(e),
    };
    Ok(Self {
      path: path.to_owned(),
      state: tokio::sync::Mutex::new(state),
      next_device_id: AtomicI32::new(1),
      dirty: AtomicBool::new(false),
    })
  }

  /// Whether a device has been paired (keys exist), without connecting.
  pub fn paired(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.len() > 0)
  }

  /// Write the state when it changed since the last flush.
  pub async fn flush(&self) -> std::io::Result<()> {
    if !self.dirty.swap(false, Ordering::AcqRel) {
      return Ok(());
    }
    let bytes = {
      let state = self.state.lock().await;
      serde_json::to_vec(&*state).map_err(std::io::Error::other)?
    };
    write_private(&self.path, &bytes).inspect_err(|_| self.dirty.store(true, Ordering::Release))
  }

  /// [`flush`](Self::flush) without awaiting, for drop paths; skipped when
  /// the state is locked.
  pub fn flush_blocking(&self) -> std::io::Result<()> {
    if !self.dirty.load(Ordering::Acquire) {
      return Ok(());
    }
    let Ok(state) = self.state.try_lock() else {
      return Ok(());
    };
    let bytes = serde_json::to_vec(&*state).map_err(std::io::Error::other)?;
    drop(state);
    self.dirty.store(false, Ordering::Release);
    write_private(&self.path, &bytes)
  }

  async fn write(&self) -> tokio::sync::MutexGuard<'_, State> {
    self.dirty.store(true, Ordering::Release);
    self.state.lock().await
  }

  async fn read(&self) -> tokio::sync::MutexGuard<'_, State> {
    self.state.lock().await
  }
}

/// Atomic, owner-only write.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
  if let Some(dir) = path.parent() {
    std::fs::create_dir_all(dir)?;
  }
  let tmp = path.with_extension("tmp");
  #[cfg(unix)]
  {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
      .write(true)
      .create(true)
      .truncate(true)
      .mode(0o600)
      .open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
  }
  #[cfg(not(unix))]
  std::fs::write(&tmp, bytes)?;
  std::fs::rename(&tmp, path)
}

fn io(e: std::io::Error) -> StoreError {
  StoreError::Io(e)
}
