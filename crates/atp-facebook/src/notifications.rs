//! The logged-in account's notifications.

use atp_core::{Notification, Page, Result, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::refs::HOME;
use crate::relay::{edges, find_key};

pub async fn list(api: &Api) -> Result<Page<Notification>> {
  api.require_session("notifications")?;
  let page = api.page(&format!("{HOME}/notifications")).await?;
  page.check("notifications", &[])?;
  let mut out: Vec<Notification> = Vec::new();
  for doc in page.docs.values() {
    let Some(m) = find_key(doc, "notifications") else {
      continue;
    };
    for edge in edges(m.at("notifications")) {
      let notification = parse::notification(edge);
      if !notification.id.is_empty() && !out.iter().any(|o| o.id == notification.id) {
        out.push(notification);
      }
    }
  }
  Ok(Page::last(out))
}
