//! The message box (消息): comments received and sent, mentions in posts and
//! comments, likes, and the unread counters.

use std::collections::BTreeMap;

use atp_core::{Error, Notification, Page, PageReq, Result, Value, ValueExt};

use crate::api::Api;
use crate::parse;

/// `notifications --type` values; the first is the default.
pub const KINDS: &[&str] = &[
  "comments",
  "mentions",
  "comment-mentions",
  "sent-comments",
  "likes",
];

pub async fn list(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
  api.require_login()?;
  // (endpoint, list key, notification kind)
  let (path, key, label) = match kind.unwrap_or("comments") {
    "comments" => ("/ajax/message/cmt", "comments", "comment"),
    "sent-comments" => ("/ajax/message/myCmt", "comments", "comment"),
    "mentions" => ("/ajax/statuses/mentions", "statuses", "mention"),
    "comment-mentions" => ("/ajax/comments/mentions", "comments", "comment"),
    "likes" => ("/ajax/message/attitudes", "attitudes", "like"),
    other => return Err(Error::input(format!("unknown notification type: {other}"))),
  };
  let mut query = vec![("count", "20".to_owned())];
  if let Some(cursor) = &page.cursor {
    query.push(("max_id", cursor.clone()));
  }
  let q: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
  let v = api.get(path, &q).await?;
  let data = v.at("data");
  let mut items: Vec<Notification> = data
    .list(key)
    .iter()
    .filter_map(|n| parse::notification(n, label))
    .collect();
  if kind == Some("comment-mentions") {
    for n in &mut items {
      n.kind = "mention".into();
    }
  }
  let next = data
    .str("next_cursor")
    .filter(|n| n != "0" && !items.is_empty());
  Ok(Page::new(items, next))
}

/// Unread counters of the notification bell.
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  api.require_login()?;
  let resp = api
    .ctx
    .http
    .get("https://rm.api.weibo.com/2/remind/push_count.json")
    .header("referer", crate::api::HOME)
    .queries([
      ("trim_null", "1"),
      ("with_dm_group", "1"),
      ("with_reminding", "1"),
      ("with_settings", "1"),
      ("exclude_attitude", "0"),
      ("with_chat_group_notice", "1"),
      ("source", "339644097"),
      ("with_chat_group", "1"),
      ("with_dm_unread", "1"),
    ])
    .send()
    .await?;
  let text = resp.text();
  // JSON, or JSONP when the server wraps it anyway.
  let body = match (text.find('{'), text.rfind('}')) {
    (Some(i), Some(j)) if j > i => &text[i..=j],
    _ => text.as_str(),
  };
  let v: Value = serde_json::from_str(body)
    .map_err(|_| Error::upstream("unexpected answer from Weibo's unread counter"))?;
  if v.i64("code").is_some_and(|code| code != 1) {
    return Err(
      Error::auth("Weibo did not accept this session for the unread counter")
        .with_hint(api.ctx.login_hint()),
    );
  }
  let data = if v.at("data").is_object() {
    v.at("data")
  } else {
    &v
  };
  let mut counts = BTreeMap::new();
  if let Some(obj) = data.as_object() {
    for (k, val) in obj {
      if let Some(n) = val.as_u64() {
        counts.insert(k.clone(), n);
      }
    }
  }
  Ok(counts)
}
