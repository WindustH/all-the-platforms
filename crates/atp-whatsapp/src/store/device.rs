//! The device record itself.

use std::sync::atomic::Ordering;
use whatsapp_rust::async_trait;
use whatsapp_rust::wacore::store::Device;
use whatsapp_rust::wacore::store::error::Result;

use super::*;

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
