//! The inbox: comment and post replies, mentions and private messages.
//! Listing it never marks anything as read (`mark=false`).

use std::collections::BTreeMap;

use social_core::{Action, Error, Notification, Page, PageReq, Result, ValueExt};

use crate::api::Api;
use crate::{listings, parse};

pub const KINDS: &[&str] = &[
  "all",
  "unread",
  "messages",
  "comments",
  "post-replies",
  "mentions",
];

pub async fn list(api: &Api, kind: Option<&str>, req: &PageReq) -> Result<Page<Notification>> {
  api.require_login()?;
  let path = match kind.unwrap_or("all") {
    "unread" => "/message/unread",
    "messages" => "/message/messages",
    "comments" => "/message/comments",
    "post-replies" => "/message/selfreply",
    "mentions" => "/message/mentions",
    _ => "/message/inbox",
  };
  let query = vec![("mark", "false".into())];
  listings::page(api, path, query, req, parse::notification).await
}

/// Unread items by kind (`reply`, `comment`, `mention`, `message`) and in total.
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  api.require_login()?;
  let query = [("mark", "false".into()), ("limit", "100".into())];
  let v = api.get("/message/unread", &query).await?;
  let mut counts = BTreeMap::from([("total".to_owned(), 0)]);
  for n in v
    .list("data.children")
    .iter()
    .filter_map(parse::notification)
  {
    *counts.entry(n.kind).or_default() += 1;
    *counts.entry("total".into()).or_default() += 1;
  }
  Ok(counts)
}

/// Mark the whole inbox as read.
pub async fn mark_read(api: &Api) -> Result<Action> {
  api.post("/api/read_all_messages", Vec::new()).await?;
  Ok(Action::done("mark-read", "inbox"))
}

/// Mark inbox items (`t1_` comments, `t4_` messages) read or unread.
/// Mark inbox items (`t1_…` / `t4_…` ids from `notifications`) read, or
/// unread with `unread`.
pub async fn mark(api: &Api, ids: &[String], unread: bool) -> Result<Action> {
  for id in ids {
    if !(id.starts_with("t1_") || id.starts_with("t4_")) {
      return Err(Error::input(format!(
        "not an inbox item id: {id} (expected t1_… or t4_…)"
      )));
    }
  }
  let (path, name) = if unread {
    ("/api/unread_message", "mark-unread")
  } else {
    ("/api/read_message", "mark-read")
  };
  api.post(path, vec![("id", ids.join(","))]).await?;
  Ok(Action::done(name, ids.join(",")))
}
