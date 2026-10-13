//! Notifications: the web's recent list, the app's typed timelines, and
//! unread counters.

use std::collections::BTreeMap;

use social_core::{Error, Notification, Page, PageReq, Result, ValueExt};

use crate::api::{Api, MOBILE, V4};
use crate::{api, parse};

/// Kinds for `notifications --type`.
pub const KINDS: &[&str] = &["all", "comment", "like", "favorite", "follow", "message"];

pub async fn notifications(
  api: &Api,
  kind: Option<&str>,
  page: &PageReq,
) -> Result<Page<Notification>> {
  let path = match kind.unwrap_or("all") {
    "all" => return recent(api, page).await,
    "message" => "/notifications/v3/message/v3".to_owned(),
    "favorite" => "/notifications/v3/timeline/entry/favlist_me".to_owned(),
    k @ ("comment" | "like" | "follow") => format!("/notifications/v3/timeline/entry/{k}"),
    other => {
      return Err(Error::input(format!(
        "unknown notification type `{other}`; one of {}",
        KINDS.join(", ")
      )));
    }
  };
  let req = match &page.cursor {
    Some(query) => api.mobile(&format!("{MOBILE}{path}?{query}")),
    None => api
      .mobile(&path)
      .query("limit", page.size_within(20).max(10)),
  };
  let v = api.call(req).await?;
  let items = v.list("data").iter().filter_map(parse::card).collect();
  Ok(Page::new(items, api::next_query(&v)))
}

pub async fn recent(api: &Api, page: &PageReq) -> Result<Page<Notification>> {
  let v = api
    .list("notifications/v2/recent?entry_name=all", None, page)
    .await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::notification).collect(),
    api::next_param(&v, "offset"),
  ))
}

/// Unread counters, as reported on the logged-in account.
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  let v = api.call(api.get(&format!("{V4}/me")).query(
      "include",
      "default_notifications_count,follow_notifications_count,vote_thank_notifications_count,messages_count",
    ),
  )
  .await?;
  Ok(
    [
      ("notifications", "default_notifications_count"),
      ("follows", "follow_notifications_count"),
      ("likes", "vote_thank_notifications_count"),
      ("messages", "messages_count"),
    ]
    .into_iter()
    .filter_map(|(key, field)| v.count(field).map(|n| (key.to_owned(), n)))
    .collect(),
  )
}
