//! Signal protocol state: identities, sessions, pre-keys, signed pre-keys
//! and sender keys.

use std::sync::Arc;
use whatsapp_rust::async_trait;
use whatsapp_rust::bytes::Bytes;
use whatsapp_rust::wacore::store::error::Result;

use super::*;

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
