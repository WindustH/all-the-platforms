//! Notifications and unread counters.

use std::collections::BTreeMap;

use atp_core::{Error, Notification, Page, PageReq, Result, ValueExt};

use crate::api::Api;
use crate::parse;

/// `notifications --type` values; the first is the default.
pub const KINDS: &[&str] = &["mentions", "likes", "connections"];

pub async fn notifications(
  api: &Api,
  kind: Option<&str>,
  page: &PageReq,
) -> Result<Page<Notification>> {
  api.require_login()?;
  let (path, fallback) = match kind.unwrap_or(KINDS[0]) {
    "mentions" => ("/api/sns/web/v1/you/mentions", "mention"),
    "likes" => ("/api/sns/web/v1/you/likes", "like"),
    "connections" => ("/api/sns/web/v1/you/connections", "follow"),
    other => return Err(Error::input(format!("unknown notification type `{other}`"))),
  };
  let num = page.size_within(20).to_string();
  let cursor = page.cursor.clone().unwrap_or_default();
  let data = api
    .get(path, &[("num", num.as_str()), ("cursor", cursor.as_str())])
    .await?;
  let items = data
    .list("message_list")
    .iter()
    .map(|m| parse::notification(m, fallback))
    .collect();
  let next = data
    .first_str(&["cursor", "str_cursor", "strCursor"])
    .filter(|_| data.bool("has_more") == Some(true));
  Ok(Page::new(items, next))
}

/// Mark one notification tab (or all of them) as read.
pub async fn mark_read(api: &Api, kind: Option<&str>) -> Result<atp_core::Action> {
  api.require_login()?;
  let kinds: Vec<&str> = match kind {
    Some(k) if KINDS.contains(&k) => vec![k],
    Some(other) => return Err(Error::input(format!("unknown notification type `{other}`"))),
    None => KINDS.to_vec(),
  };
  for k in &kinds {
    api
      .post(
        "/api/sns/web/v1/message/read",
        &atp_core::json!({"type": k}),
      )
      .await?;
  }
  Ok(atp_core::Action::done("mark-read", kinds.join(",")))
}

/// `unread_count`, `mentions`, `likes`, `connections` and any other counter.
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  api.require_login()?;
  let data = api.get("/api/sns/web/unread_count", &[]).await?;
  Ok(
    data
      .as_object()
      .map(|m| {
        m.iter()
          .filter_map(|(k, v)| Some((k.clone(), v.as_u64()?)))
          .collect()
      })
      .unwrap_or_default(),
  )
}
