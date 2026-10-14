//! Unread counts of the creator center's notifications.

use std::collections::BTreeMap;

use atp_core::{Result, ValueExt};

use crate::api::Api;
use crate::login;

const UNREAD: &str = "/rest/v2/creator/pc/notification/unReadCountV3";

pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  api.require_login()?;
  login::cp_tickets(api).await?;
  let v = api.cp(UNREAD).send().await?;
  Ok(
    v.at("data")
      .as_object()
      .map(|m| {
        m.iter()
          .filter_map(|(k, v)| Some((k.clone(), v.as_u64()?)))
          .collect()
      })
      .unwrap_or_default(),
  )
}
