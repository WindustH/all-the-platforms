//! App state sync: keys, hash states and mutation MACs.

use whatsapp_rust::async_trait;
use whatsapp_rust::wacore::appstate::hash::HashState;
use whatsapp_rust::wacore::appstate::processor::AppStateMutationMAC;
use whatsapp_rust::wacore::store::error::Result;

use super::*;

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
