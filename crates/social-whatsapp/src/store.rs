//! The linked device's keys and protocol state, kept in one private file.
//!
//! whatsapp-rust stores sessions through its `Backend` trait; its SQLite
//! backend cannot be linked next to the browser-cookie importer (both bundle
//! `sqlite3`), so this is its in-memory backend (`wacore::store::in_memory`,
//! MIT, by João Lucas) with a serializable state: every write marks it dirty
//! and [`FileBackend::flush`] writes it out (mode 0600) at the end of a
//! command and periodically while connected.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use serde::{Deserialize, Serialize};
use whatsapp_rust::async_trait;
use whatsapp_rust::bytes::Bytes;
use whatsapp_rust::wacore::appstate::hash::HashState;
use whatsapp_rust::wacore::appstate::processor::AppStateMutationMAC;
use whatsapp_rust::wacore::store::Device;
use whatsapp_rust::wacore::store::error::{Result, StoreError};
use whatsapp_rust::wacore::store::traits::*;
use whatsapp_rust::wacore::time::now_secs;

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

#[async_trait]
impl SignalStore for FileBackend {
  async fn put_identity(&self, address: &str, key: [u8; 32]) -> Result<()> {
    self.write().await.identities.insert(address.into(), key);
    Ok(())
  }

  async fn load_identity(&self, address: &str) -> Result<Option<[u8; 32]>> {
    Ok(self.read().await.identities.get(address).copied())
  }

  async fn delete_identity(&self, address: &str) -> Result<()> {
    self.write().await.identities.remove(address);
    Ok(())
  }

  async fn get_session(&self, address: &str) -> Result<Option<Bytes>> {
    Ok(
      self
        .read()
        .await
        .sessions
        .get(address)
        .cloned()
        .map(Bytes::from),
    )
  }

  async fn put_session(&self, address: &str, session: &[u8]) -> Result<()> {
    self
      .write()
      .await
      .sessions
      .insert(address.into(), session.to_vec());
    Ok(())
  }

  async fn put_sessions_batch(&self, sessions: &[(Arc<str>, Bytes)]) -> Result<()> {
    let mut s = self.write().await;
    for (address, session) in sessions {
      s.sessions.insert(address.to_string(), session.to_vec());
    }
    Ok(())
  }

  async fn has_session(&self, address: &str) -> Result<bool> {
    Ok(self.read().await.sessions.contains_key(address))
  }

  async fn has_signal_state_for_user(&self, user: &str) -> Result<bool> {
    fn matches(addr: &str, user: &str) -> bool {
      addr
        .strip_prefix(user)
        .is_some_and(|rest| rest.starts_with('@') || rest.starts_with(':'))
    }
    let s = self.read().await;
    Ok(s.sessions.keys().any(|k| matches(k, user)) || s.identities.keys().any(|k| matches(k, user)))
  }

  async fn delete_session(&self, address: &str) -> Result<()> {
    self.write().await.sessions.remove(address);
    Ok(())
  }

  async fn store_prekey(&self, id: u32, record: &[u8], _uploaded: bool) -> Result<()> {
    self.write().await.prekeys.insert(id, record.to_vec());
    Ok(())
  }

  async fn mark_prekeys_uploaded(&self, _ids: &[u32]) -> Result<()> {
    Ok(())
  }

  async fn store_prekeys_batch(&self, keys: &[(u32, Bytes)], _uploaded: bool) -> Result<()> {
    let mut s = self.write().await;
    for (id, record) in keys {
      s.prekeys.insert(*id, record.to_vec());
    }
    Ok(())
  }

  async fn load_prekey(&self, id: u32) -> Result<Option<Bytes>> {
    Ok(self.read().await.prekeys.get(&id).cloned().map(Bytes::from))
  }

  async fn load_prekeys_batch(&self, ids: &[u32]) -> Result<Vec<(u32, Bytes)>> {
    let s = self.read().await;
    Ok(
      ids
        .iter()
        .filter_map(|id| s.prekeys.get(id).map(|r| (*id, Bytes::from(r.clone()))))
        .collect(),
    )
  }

  async fn remove_prekey(&self, id: u32) -> Result<()> {
    self.write().await.prekeys.remove(&id);
    Ok(())
  }

  async fn get_max_prekey_id(&self) -> Result<u32> {
    Ok(self.read().await.prekeys.keys().copied().max().unwrap_or(0))
  }

  async fn store_signed_prekey(&self, id: u32, record: &[u8]) -> Result<()> {
    self
      .write()
      .await
      .signed_prekeys
      .insert(id, record.to_vec());
    Ok(())
  }

  async fn load_signed_prekey(&self, id: u32) -> Result<Option<Vec<u8>>> {
    Ok(self.read().await.signed_prekeys.get(&id).cloned())
  }

  async fn load_all_signed_prekeys(&self) -> Result<Vec<(u32, Vec<u8>)>> {
    Ok(
      self
        .read()
        .await
        .signed_prekeys
        .iter()
        .map(|(id, r)| (*id, r.clone()))
        .collect(),
    )
  }

  async fn remove_signed_prekey(&self, id: u32) -> Result<()> {
    self.write().await.signed_prekeys.remove(&id);
    Ok(())
  }

  async fn put_sender_key(&self, address: &str, record: &[u8]) -> Result<()> {
    self
      .write()
      .await
      .sender_keys
      .insert(address.into(), record.to_vec());
    Ok(())
  }

  async fn put_sender_keys_batch(&self, sender_keys: &[(Arc<str>, Bytes)]) -> Result<()> {
    let mut s = self.write().await;
    for (address, record) in sender_keys {
      s.sender_keys.insert(address.to_string(), record.to_vec());
    }
    Ok(())
  }

  async fn get_sender_key(&self, address: &str) -> Result<Option<Vec<u8>>> {
    Ok(self.read().await.sender_keys.get(address).cloned())
  }

  async fn delete_sender_key(&self, address: &str) -> Result<()> {
    self.write().await.sender_keys.remove(address);
    Ok(())
  }
}

#[async_trait]
impl AppSyncStore for FileBackend {
  async fn get_sync_key(&self, key_id: &[u8]) -> Result<Option<AppStateSyncKey>> {
    let s = self.read().await;
    Ok(
      s.sync_keys
        .iter()
        .find(|(id, _)| id == key_id)
        .map(|(_, k)| k.clone()),
    )
  }

  async fn set_sync_key(&self, key_id: &[u8], key: AppStateSyncKey) -> Result<()> {
    let mut s = self.write().await;
    s.sync_keys.retain(|(id, _)| id != key_id);
    s.sync_keys.push((key_id.to_vec(), key));
    s.latest_sync_key_id = Some(key_id.to_vec());
    Ok(())
  }

  async fn get_version(&self, name: &str) -> Result<HashState> {
    Ok(
      self
        .read()
        .await
        .versions
        .get(name)
        .cloned()
        .unwrap_or_default(),
    )
  }

  async fn set_version(&self, name: &str, state: HashState) -> Result<()> {
    self.write().await.versions.insert(name.into(), state);
    Ok(())
  }

  async fn put_mutation_macs(
    &self,
    name: &str,
    _version: u64,
    mutations: &[AppStateMutationMAC],
  ) -> Result<()> {
    let mut s = self.write().await;
    for m in mutations {
      s.mutation_macs
        .retain(|(n, index, _)| !(n == name && *index == m.index_mac));
      s.mutation_macs
        .push((name.into(), m.index_mac.clone(), m.value_mac.clone()));
    }
    Ok(())
  }

  async fn get_mutation_mac(&self, name: &str, index_mac: &[u8]) -> Result<Option<Vec<u8>>> {
    let s = self.read().await;
    Ok(
      s.mutation_macs
        .iter()
        .find(|(n, index, _)| n == name && index == index_mac)
        .map(|(_, _, value)| value.clone()),
    )
  }

  async fn delete_mutation_macs(&self, name: &str, index_macs: &[Vec<u8>]) -> Result<()> {
    self
      .write()
      .await
      .mutation_macs
      .retain(|(n, index, _)| !(n == name && index_macs.contains(index)));
    Ok(())
  }

  async fn clear_mutation_macs(&self, name: &str) -> Result<()> {
    self
      .write()
      .await
      .mutation_macs
      .retain(|(n, _, _)| n != name);
    Ok(())
  }

  async fn get_latest_sync_key_id(&self) -> Result<Option<Vec<u8>>> {
    Ok(self.read().await.latest_sync_key_id.clone())
  }
}

#[async_trait]
impl ProtocolStore for FileBackend {
  async fn get_sender_key_devices(&self, group_jid: &str) -> Result<Vec<(String, bool)>> {
    Ok(
      self
        .read()
        .await
        .sender_key_devices
        .get(group_jid)
        .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
        .unwrap_or_default(),
    )
  }

  async fn set_sender_key_status(&self, group_jid: &str, entries: &[(&str, bool)]) -> Result<()> {
    let mut s = self.write().await;
    let map = s.sender_key_devices.entry(group_jid.into()).or_default();
    for (device, has_key) in entries {
      map.insert((*device).into(), *has_key);
    }
    Ok(())
  }

  async fn clear_sender_key_devices(&self, group_jid: &str) -> Result<()> {
    self.write().await.sender_key_devices.remove(group_jid);
    Ok(())
  }

  async fn clear_all_sender_key_devices(&self) -> Result<()> {
    self.write().await.sender_key_devices.clear();
    Ok(())
  }

  async fn delete_sender_key_device_rows(&self, device_jids: &[&str]) -> Result<()> {
    if device_jids.is_empty() {
      return Ok(());
    }
    let mut s = self.write().await;
    for map in s.sender_key_devices.values_mut() {
      map.retain(|jid, _| !device_jids.contains(&jid.as_str()));
    }
    Ok(())
  }

  async fn get_lid_mapping(&self, lid: &str) -> Result<Option<LidPnMappingEntry>> {
    Ok(self.read().await.lid_mappings.get(lid).cloned())
  }

  async fn get_pn_mapping(&self, phone: &str) -> Result<Option<LidPnMappingEntry>> {
    let s = self.read().await;
    Ok(
      s.pn_to_lid
        .get(phone)
        .and_then(|lid| s.lid_mappings.get(lid))
        .cloned(),
    )
  }

  async fn put_lid_mapping(&self, entry: &LidPnMappingEntry) -> Result<()> {
    let mut s = self.write().await;
    if let Some(old) = s
      .lid_mappings
      .get(&entry.lid)
      .filter(|old| old.phone_number != entry.phone_number)
      .map(|old| old.phone_number.clone())
    {
      s.pn_to_lid.remove(&old);
    }
    s.pn_to_lid
      .insert(entry.phone_number.clone(), entry.lid.clone());
    s.lid_mappings.insert(entry.lid.clone(), entry.clone());
    Ok(())
  }

  async fn get_all_lid_mappings(&self) -> Result<Vec<LidPnMappingEntry>> {
    Ok(self.read().await.lid_mappings.values().cloned().collect())
  }

  async fn save_base_key(&self, address: &str, message_id: &str, base_key: &[u8]) -> Result<()> {
    let mut s = self.write().await;
    s.base_keys
      .retain(|(a, m, _)| !(a == address && m == message_id));
    s.base_keys
      .push((address.into(), message_id.into(), base_key.to_vec()));
    Ok(())
  }

  async fn has_same_base_key(
    &self,
    address: &str,
    message_id: &str,
    current_base_key: &[u8],
  ) -> Result<bool> {
    Ok(
      self
        .read()
        .await
        .base_keys
        .iter()
        .any(|(a, m, k)| a == address && m == message_id && k == current_base_key),
    )
  }

  async fn delete_base_key(&self, address: &str, message_id: &str) -> Result<()> {
    self
      .write()
      .await
      .base_keys
      .retain(|(a, m, _)| !(a == address && m == message_id));
    Ok(())
  }

  async fn update_device_list(&self, record: DeviceListRecord) -> Result<()> {
    self
      .write()
      .await
      .device_lists
      .insert(record.user.clone(), record);
    Ok(())
  }

  async fn get_devices(&self, user: &str) -> Result<Option<DeviceListRecord>> {
    Ok(self.read().await.device_lists.get(user).cloned())
  }

  async fn delete_devices(&self, user: &str) -> Result<()> {
    self.write().await.device_lists.remove(user);
    Ok(())
  }

  async fn get_group_metadata(&self, group_jid: &str) -> Result<Option<Vec<u8>>> {
    Ok(self.read().await.group_metadata.get(group_jid).cloned())
  }

  async fn put_group_metadata(&self, group_jid: &str, blob: &[u8]) -> Result<()> {
    self
      .write()
      .await
      .group_metadata
      .insert(group_jid.into(), blob.to_vec());
    Ok(())
  }

  async fn delete_group_metadata(&self, group_jid: &str) -> Result<()> {
    self.write().await.group_metadata.remove(group_jid);
    Ok(())
  }

  async fn get_tc_token(&self, jid: &str) -> Result<Option<TcTokenEntry>> {
    Ok(self.read().await.tc_tokens.get(jid).cloned())
  }

  async fn put_tc_token(&self, jid: &str, entry: &TcTokenEntry) -> Result<()> {
    self
      .write()
      .await
      .tc_tokens
      .insert(jid.into(), entry.clone());
    Ok(())
  }

  async fn delete_tc_token(&self, jid: &str) -> Result<()> {
    self.write().await.tc_tokens.remove(jid);
    Ok(())
  }

  async fn get_all_tc_token_jids(&self) -> Result<Vec<String>> {
    Ok(self.read().await.tc_tokens.keys().cloned().collect())
  }

  async fn delete_expired_tc_tokens(&self, token_cutoff: i64, sender_cutoff: i64) -> Result<u32> {
    let mut s = self.write().await;
    let before = s.tc_tokens.len();
    s.tc_tokens.retain(|_, e| {
      let token_live = !e.token.is_empty() && e.token_timestamp >= token_cutoff;
      let sender_live = e.sender_timestamp.is_some_and(|ts| ts >= sender_cutoff);
      token_live || sender_live
    });
    Ok((before - s.tc_tokens.len()) as u32)
  }

  async fn touch_tc_token_sender_timestamp(&self, jid: &str, sender_timestamp: i64) -> Result<()> {
    let mut s = self.write().await;
    match s.tc_tokens.get_mut(jid) {
      Some(e) => {
        e.sender_timestamp = Some(
          e.sender_timestamp
            .map_or(sender_timestamp, |t| t.max(sender_timestamp)),
        );
      }
      None => {
        s.tc_tokens.insert(
          jid.into(),
          TcTokenEntry {
            token: Vec::new(),
            token_timestamp: sender_timestamp,
            sender_timestamp: Some(sender_timestamp),
          },
        );
      }
    }
    Ok(())
  }

  async fn store_received_tc_token(
    &self,
    jid: &str,
    token: &[u8],
    token_timestamp: i64,
  ) -> Result<()> {
    let mut s = self.write().await;
    match s.tc_tokens.get_mut(jid) {
      Some(e) => {
        if e.token.is_empty() || token_timestamp >= e.token_timestamp {
          e.token = token.to_vec();
          e.token_timestamp = token_timestamp;
        }
      }
      None => {
        s.tc_tokens.insert(
          jid.into(),
          TcTokenEntry {
            token: token.to_vec(),
            token_timestamp,
            sender_timestamp: None,
          },
        );
      }
    }
    Ok(())
  }

  async fn store_sent_message(
    &self,
    chat_jid: &str,
    message_id: &str,
    payload: &[u8],
  ) -> Result<()> {
    let now = now_secs();
    let mut s = self.write().await;
    if s.sent_messages.len() >= MAX_SENT_MESSAGES {
      s.sent_messages
        .sort_by_key(|(.., at)| std::cmp::Reverse(*at));
      s.sent_messages.truncate(MAX_SENT_MESSAGES * 3 / 4);
    }
    s.sent_messages
      .retain(|(c, m, ..)| !(c == chat_jid && m == message_id));
    s.sent_messages
      .push((chat_jid.into(), message_id.into(), payload.to_vec(), now));
    Ok(())
  }

  async fn take_sent_message(&self, chat_jid: &str, message_id: &str) -> Result<Option<Vec<u8>>> {
    let mut s = self.write().await;
    let pos = s
      .sent_messages
      .iter()
      .position(|(c, m, ..)| c == chat_jid && m == message_id);
    Ok(pos.map(|i| s.sent_messages.swap_remove(i).2))
  }

  async fn delete_expired_sent_messages(&self, cutoff_timestamp: i64) -> Result<u32> {
    let mut s = self.write().await;
    let before = s.sent_messages.len();
    s.sent_messages.retain(|(.., at)| *at >= cutoff_timestamp);
    Ok((before - s.sent_messages.len()) as u32)
  }

  async fn store_pending_inbound(
    &self,
    chat: &str,
    sender: &str,
    id: &str,
    message: &[u8],
  ) -> Result<()> {
    let now = now_secs();
    let mut s = self.write().await;
    s.pending_inbound
      .retain(|(c, f, i, ..)| !(c == chat && f == sender && i == id));
    s.pending_inbound
      .push((chat.into(), sender.into(), id.into(), message.to_vec(), now));
    Ok(())
  }

  async fn get_pending_inbound(
    &self,
    chat: &str,
    sender: &str,
    id: &str,
  ) -> Result<Option<Vec<u8>>> {
    Ok(
      self
        .read()
        .await
        .pending_inbound
        .iter()
        .find(|(c, f, i, ..)| c == chat && f == sender && i == id)
        .map(|(.., m, _)| m.clone()),
    )
  }

  async fn delete_pending_inbound(&self, chat: &str, sender: &str, id: &str) -> Result<()> {
    self
      .write()
      .await
      .pending_inbound
      .retain(|(c, f, i, ..)| !(c == chat && f == sender && i == id));
    Ok(())
  }

  async fn delete_expired_pending_inbound(&self, cutoff_timestamp: i64) -> Result<u32> {
    let mut s = self.write().await;
    let before = s.pending_inbound.len();
    s.pending_inbound.retain(|(.., at)| *at >= cutoff_timestamp);
    Ok((before - s.pending_inbound.len()) as u32)
  }
}

#[async_trait]
impl MsgSecretStore for FileBackend {
  async fn put_msg_secrets(&self, entries: Vec<MsgSecretEntry>) -> Result<usize> {
    let stored = entries.len();
    let mut s = self.write().await;
    let mut index: HashMap<(String, String, String), usize> = s
      .msg_secrets
      .iter()
      .enumerate()
      .map(|(i, (c, f, m, ..))| ((c.clone(), f.clone(), m.clone()), i))
      .collect();
    for e in entries {
      let key = (
        e.chat.to_string(),
        e.sender.to_string(),
        e.msg_id.to_string(),
      );
      match index.get(&key) {
        Some(&i) => {
          let row = &mut s.msg_secrets[i];
          row.3 = e.secret.to_vec();
          row.4 = merge_msg_secret_expiry(row.4, e.expires_at);
          row.5 = merge_msg_secret_message_ts(row.5, e.message_ts);
        }
        None => {
          index.insert(key.clone(), s.msg_secrets.len());
          s.msg_secrets.push((
            key.0,
            key.1,
            key.2,
            e.secret.to_vec(),
            e.expires_at,
            e.message_ts,
          ));
        }
      }
    }
    Ok(stored)
  }

  async fn get_msg_secret(
    &self,
    chat: &str,
    sender: &str,
    msg_id: &str,
  ) -> Result<Option<Vec<u8>>> {
    Ok(
      self
        .get_msg_secret_with_ts(chat, sender, msg_id)
        .await?
        .map(|(secret, _)| secret),
    )
  }

  async fn get_msg_secret_with_ts(
    &self,
    chat: &str,
    sender: &str,
    msg_id: &str,
  ) -> Result<Option<(Vec<u8>, i64)>> {
    Ok(
      self
        .read()
        .await
        .msg_secrets
        .iter()
        .find(|(c, f, m, ..)| c == chat && f == sender && m == msg_id)
        .map(|(.., secret, _, ts)| (secret.clone(), *ts)),
    )
  }

  async fn delete_expired_msg_secrets(&self, cutoff_timestamp: i64) -> Result<u32> {
    let mut s = self.write().await;
    let before = s.msg_secrets.len();
    s.msg_secrets
      .retain(|(.., expires, _)| *expires == 0 || *expires > cutoff_timestamp);
    Ok((before - s.msg_secrets.len()) as u32)
  }
}

#[async_trait]
impl DeviceStore for FileBackend {
  async fn save(&self, device: &Device) -> Result<()> {
    self.write().await.device = Some(device.clone());
    // The device record is what pairing produces: never leave it unsaved.
    self.flush().await.map_err(io)
  }

  async fn load(&self) -> Result<Option<Device>> {
    Ok(self.read().await.device.clone())
  }

  async fn exists(&self) -> Result<bool> {
    Ok(self.read().await.device.is_some())
  }

  async fn create(&self) -> Result<i32> {
    let id = self.next_device_id.fetch_add(1, Ordering::Relaxed);
    let mut s = self.write().await;
    if s.device.is_none() {
      s.device = Some(Device::new());
    }
    Ok(id)
  }
}
