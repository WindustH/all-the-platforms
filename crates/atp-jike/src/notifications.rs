//! Notifications (`/1.0/notifications/list`) and the unread count
//! (`/1.0/notifications/unread`).

use std::collections::BTreeMap;

use atp_core::{Notification, Page, PageReq, Result, ValueExt, json};

use crate::api::Api;
use crate::parse;

pub async fn list(api: &Api, req: &PageReq) -> Result<Page<Notification>> {
  api.require_login()?;
  let (v, next) = api.list("/1.0/notifications/list", json!({}), req).await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::notification).collect(),
    next,
  ))
}

pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  api.require_login()?;
  let v = api.get("/1.0/notifications/unread", &[]).await?;
  Ok(BTreeMap::from([(
    "notifications".to_owned(),
    v.u64("unreadCount").unwrap_or_default(),
  )]))
}
