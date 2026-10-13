//! Message secrets (for edits, reactions and polls).

use std::collections::HashMap;
use whatsapp_rust::async_trait;
use whatsapp_rust::wacore::store::error::Result;

use super::*;

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
