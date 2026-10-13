//! The logged-in account's inbox and saved threads: replies, @ mentions and
//! agrees (点赞), unread counters, and 收藏.

use std::collections::BTreeMap;

use social_core::{Error, Notification, Page, PageReq, Post, Result, ValueExt};

use crate::api::{Api, LEGACY, Params};
use crate::parse;

/// `notifications --type` values: replies to you, @ mentions, agrees.
pub const KINDS: &[&str] = &["reply", "at", "agree"];

pub async fn notifications(
  api: &Api,
  kind: Option<&str>,
  req: &PageReq,
) -> Result<Page<Notification>> {
  api.require_login()?;
  let kind = kind.unwrap_or("reply");
  let path = match kind {
    "reply" => "/c/u/feed/replyme",
    "at" => "/c/u/feed/atme",
    "agree" => "/c/u/feed/agreeme",
    other => {
      return Err(Error::input(format!(
        "unknown notification type {other}; one of {}",
        KINDS.join(", ")
      )));
    }
  };
  let pn = req.number_or(1);
  let v = api
    .call_as(path, vec![("pn", pn.to_string())], LEGACY)
    .await?;
  let list = ["reply_list", "at_list", "agree_list", "list"]
    .iter()
    .map(|k| v.list(k))
    .find(|l| !l.is_empty())
    .unwrap_or_default();
  let items: Vec<Notification> = list.iter().map(|m| parse::notification(m, kind)).collect();
  let more = v.str("page.has_more").as_deref() == Some("1") && !items.is_empty();
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}

/// Unread counters: replies, @ mentions, agrees, new fans, saved-thread updates.
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  api.require_login()?;
  let v = api
    .call_as("/c/s/msg", vec![("bookmark", "1".into())], LEGACY)
    .await?;
  let message = v.at("message");
  let mut out = BTreeMap::new();
  for (key, name) in [
    ("replyme", "reply"),
    ("atme", "at"),
    ("agree", "agree"),
    ("fans", "followers"),
    ("storethread", "favorites"),
    ("recycle", "recycle"),
  ] {
    if let Some(n) = message.u64(key) {
      out.insert(name.to_owned(), n);
    }
  }
  Ok(out)
}

/// One page of your saved threads (收藏).
pub async fn favorites(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  let me = api.me().await?;
  let offset = req.number_or(0);
  let rn = req.size_within(50);
  let params: Params = vec![
    ("rn", rn.to_string()),
    ("offset", offset.to_string()),
    ("user_id", me.uid),
    ("stoken", api.stoken()),
  ];
  let v = api.call("/c/f/post/threadstore", params).await?;
  let none = parse::Users::new();
  let list = v.list("store_thread");
  let items: Vec<Post> = list.iter().map(|t| parse::thread(t, &none)).collect();
  let more = list.len() >= rn;
  Ok(Page::new(
    items,
    more.then(|| (offset + rn as u64).to_string()),
  ))
}
