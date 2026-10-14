//! Users: you (`/session/current.json`), profiles with their summary,
//! their topics and replies, your bookmarks, notifications and unread
//! counts.

use std::collections::BTreeMap;

use atp_core::{Error, Notification, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse::{Forum, notification_kind};
use crate::refs;

/// Kinds `notifications --type` filters by.
pub const NOTIFICATION_KINDS: &[&str] = &[
  "mention", "reply", "quote", "like", "message", "post", "linked", "badge",
];
/// Activity rows per request.
const ACTIONS_PAGE: u64 = 30;

/// `/session/current.json`: who the credentials act as.
pub async fn current(api: &Api) -> Result<Value> {
  api.require()?;
  let v = api.get("/session/current.json", Vec::new()).await?;
  let user = v.at("current_user");
  if !user.is_object() {
    return Err(Error::auth("the forum does not know these credentials"));
  }
  Ok(user.clone())
}

pub async fn whoami(api: &Api) -> Result<User> {
  let v = current(api).await?;
  let base = api.forum();
  Ok(Forum { base: &base }.user_brief(&v).unwrap_or_default())
}

fn name(api: &Api, arg: &str) -> Result<String> {
  refs::user(arg, api.username().as_deref())
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let name = name(api, arg)?;
  let v = api.get(&format!("/u/{name}.json"), Vec::new()).await?;
  let summary = api
    .get(&format!("/u/{name}/summary.json"), Vec::new())
    .await
    .unwrap_or_default();
  let base = api.forum();
  Ok(Forum { base: &base }.user(v.at("user"), summary.at("user_summary")))
}

/// Topics and replies a user wrote (`/user_actions.json`, filters 4 and 5).
pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let name = name(api, arg)?;
  let offset = page.number_or(0);
  let query = vec![
    ("username", name),
    ("filter", "4,5".to_owned()),
    ("offset", offset.to_string()),
  ];
  let v = api.get("/user_actions.json", query).await?;
  let base = api.forum();
  let forum = Forum { base: &base };
  let posts: Vec<Post> = v
    .list("user_actions")
    .iter()
    .map(|row| forum.post(row, None))
    .collect();
  let more = posts.len() as u64 >= ACTIONS_PAGE;
  Ok(Page::new(
    posts,
    more.then(|| (offset + ACTIONS_PAGE).to_string()),
  ))
}

/// Your bookmarks (`/u/<you>/bookmarks.json`, `page` from 0).
pub async fn bookmarks(api: &Api, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  api.require()?;
  let me = api.username();
  let name = match user {
    Some(arg) => name(api, arg)?,
    None => me
      .clone()
      .ok_or_else(|| Error::auth("the login saved no username"))?,
  };
  if Some(&name) != me.as_ref() {
    return Err(Error::unsupported("another user's bookmarks"));
  }
  let number = page.number_or(0);
  let query = vec![("page", number.to_string())];
  let v = api.get(&format!("/u/{name}/bookmarks.json"), query).await?;
  let base = api.forum();
  let forum = Forum { base: &base };
  let posts: Vec<Post> = v
    .list("user_bookmark_list.bookmarks")
    .iter()
    .map(|b| forum.post(b, b.str("title")))
    .collect();
  let more = v.str("user_bookmark_list.more_bookmarks_url").is_some() && !posts.is_empty();
  Ok(Page::new(posts, more.then(|| (number + 1).to_string())))
}

pub async fn notifications(
  api: &Api,
  kind: Option<&str>,
  page: &PageReq,
) -> Result<Page<Notification>> {
  api.require()?;
  let offset = page.number_or(0);
  let size = page.size_within(60) as u64;
  let query = vec![("offset", offset.to_string()), ("limit", size.to_string())];
  let v = api.get("/notifications.json", query).await?;
  let rows = v.list("notifications");
  let base = api.forum();
  let forum = Forum { base: &base };
  let items: Vec<Notification> = rows
    .iter()
    .filter(|n| {
      kind.is_none_or(|k| notification_kind(n.u64("notification_type").unwrap_or(0)) == k)
    })
    .map(|n| forum.notification(n))
    .collect();
  let more = rows.len() as u64 >= size;
  Ok(Page::new(
    items,
    more.then(|| (offset + rows.len() as u64).to_string()),
  ))
}

/// Unread notifications and messages, as `current_user` counts them.
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  let v = current(api).await?;
  let mut counts = BTreeMap::new();
  for (key, field) in [
    ("notifications", "unread_notifications"),
    ("high_priority", "unread_high_priority_notifications"),
    ("all", "all_unread_notifications_count"),
    ("messages", "new_personal_messages_notifications_count"),
  ] {
    if let Some(n) = v.u64(field) {
      counts.insert(key.to_owned(), n);
    }
  }
  Ok(counts)
}
