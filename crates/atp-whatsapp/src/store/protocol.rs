//! Protocol bookkeeping: sent messages, device lists, LID mappings and the
//! rest whatsapp-rust keeps between connections.

use whatsapp_rust::async_trait;
use whatsapp_rust::wacore::store::error::Result;
use whatsapp_rust::wacore::time::now_secs;

use super::*;

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
