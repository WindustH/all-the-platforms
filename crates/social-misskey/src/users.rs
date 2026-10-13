//! Users: the account, profiles, their notes, followers, search, follows,
//! notifications.

use std::collections::BTreeMap;

use social_core::{
  Action, Notification, Page, PageReq, Post, Query, Result, User, Value, ValueExt, json,
};

use crate::api::Api;
use crate::notes::paged;
use crate::parse;
use crate::refs::{self, UserRef};

pub const NOTIFICATION_KINDS: &[&str] = &[
  "mention",
  "reply",
  "renote",
  "quote",
  "reaction",
  "follow",
  "receiveFollowRequest",
  "pollEnded",
  "achievementEarned",
];

const ID_KEY: &str = "user_id";

pub async fn me(api: &Api) -> Result<Value> {
  let v = api.call_auth("i", json!({})).await?;
  if let Some(id) = v.str("id") {
    api.ctx.set_extra(ID_KEY, &id);
  }
  Ok(v)
}

/// The account's own id, from the session or `i`.
pub async fn my_id(api: &Api) -> Result<String> {
  if let Some(id) = api.ctx.extra(ID_KEY) {
    return Ok(id);
  }
  Ok(me(api).await?.str("id").unwrap_or_default())
}

pub async fn whoami(api: &Api) -> Result<User> {
  Ok(parse::user(&me(api).await?, &api.instance()))
}

pub async fn lookup(api: &Api, r: &UserRef) -> Result<Value> {
  match r {
    UserRef::Me => me(api).await,
    UserRef::Id(id) => api.call("users/show", json!({"userId": id})).await,
    UserRef::Name { username, host } => {
      api
        .call("users/show", json!({"username": username, "host": host}))
        .await
    }
  }
}

pub async fn user_id(api: &Api, arg: &str) -> Result<String> {
  match refs::user(arg, &api.instance())? {
    UserRef::Id(id) => Ok(id),
    UserRef::Me => my_id(api).await,
    r => Ok(lookup(api, &r).await?.str("id").unwrap_or_default()),
  }
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let r = refs::user(arg, &api.instance())?;
  Ok(parse::user(&lookup(api, &r).await?, &api.instance()))
}

pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = user_id(api, arg).await?;
  let size = page.size_within(100);
  let mut body = json!({"userId": id, "limit": size, "withRenotes": true, "withReplies": false});
  if let Some(c) = &page.cursor {
    body["untilId"] = json!(c);
  }
  let v = api.call("users/notes", body).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let instance = api.instance();
  let items = rows.iter().map(|n| parse::note(n, &instance)).collect();
  Ok(Page::new(items, paged(&rows, size)))
}

/// `users/followers` / `users/following`: relation rows paged by their own id.
pub async fn relations(
  api: &Api,
  arg: &str,
  followers: bool,
  page: &PageReq,
) -> Result<Page<User>> {
  let id = user_id(api, arg).await?;
  let size = page.size_within(100);
  let mut body = json!({"userId": id, "limit": size});
  if let Some(c) = &page.cursor {
    body["untilId"] = json!(c);
  }
  let (endpoint, key) = if followers {
    ("users/followers", "follower")
  } else {
    ("users/following", "followee")
  };
  let v = api.call(endpoint, body).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let instance = api.instance();
  let items = rows
    .iter()
    .map(|r| parse::user(r.at(key), &instance))
    .collect();
  Ok(Page::new(items, paged(&rows, size)))
}

pub async fn search(api: &Api, q: &Query, page: &PageReq) -> Result<Page<User>> {
  let size = page.size_within(100);
  let offset: usize = page
    .cursor
    .as_deref()
    .and_then(|c| c.parse().ok())
    .unwrap_or(0);
  let v = api
    .call(
      "users/search",
      json!({"query": q.keyword.trim_start_matches('@'), "limit": size, "offset": offset}),
    )
    .await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let instance = api.instance();
  let next = (rows.len() >= size).then(|| (offset + rows.len()).to_string());
  Ok(Page::new(
    rows.iter().map(|u| parse::user(u, &instance)).collect(),
    next,
  ))
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = user_id(api, arg).await?;
  let endpoint = if undo {
    "following/delete"
  } else {
    "following/create"
  };
  api.write(endpoint, json!({"userId": id})).await?;
  Ok(Action::done(if undo { "unfollow" } else { "follow" }, &id))
}

pub async fn notifications(
  api: &Api,
  kind: Option<&str>,
  page: &PageReq,
) -> Result<Page<Notification>> {
  let size = page.size_within(100);
  let mut body = json!({"limit": size, "markAsRead": false});
  if let Some(c) = &page.cursor {
    body["untilId"] = json!(c);
  }
  if let Some(k) = kind {
    body["includeTypes"] = json!([k]);
  }
  let v = api.call_auth("i/notifications", body).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let instance = api.instance();
  Ok(Page::new(
    rows
      .iter()
      .map(|n| parse::notification(n, &instance))
      .collect(),
    paged(&rows, size),
  ))
}

/// What `i` says is waiting: notifications, mentions, chat messages ...
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  let v = me(api).await?;
  let mut out = BTreeMap::new();
  if let Some(n) = v.u64("unreadNotificationsCount") {
    out.insert("notifications".into(), n);
  }
  for (key, field) in [
    ("notifications", "hasUnreadNotification"),
    ("mentions", "hasUnreadMentions"),
    ("specified_notes", "hasUnreadSpecifiedNotes"),
    ("chat", "hasUnreadChatMessages"),
    ("announcements", "hasUnreadAnnouncement"),
    ("antennas", "hasUnreadAntenna"),
  ] {
    if let Some(b) = v.bool(field) {
      out.entry(key.into()).or_insert(u64::from(b));
    }
  }
  Ok(out)
}
