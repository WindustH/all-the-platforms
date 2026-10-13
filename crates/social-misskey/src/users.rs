//! Users: the account, profiles, their notes, followers, follows,
//! notifications.

use std::collections::BTreeMap;

use social_core::{Action, Notification, Page, PageReq, Post, Result, User, Value, ValueExt, json};

use crate::api::Api;
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

/// The profile a reference points at, as Misskey sends it.
pub async fn lookup(api: &Api, user: &UserRef) -> Result<Value> {
  match user {
    UserRef::Me => me(api).await,
    UserRef::Id(id) => api.call("users/show", json!({"userId": id})).await,
    UserRef::Name { username, host } => {
      api
        .call("users/show", json!({"username": username, "host": host}))
        .await
    }
  }
}

/// The id of the user a reference points at.
pub async fn id(api: &Api, user: &UserRef) -> Result<String> {
  match user {
    UserRef::Id(id) => Ok(id.clone()),
    UserRef::Me => my_id(api).await,
    user => Ok(lookup(api, user).await?.str("id").unwrap_or_default()),
  }
}

async fn user_id(api: &Api, arg: &str) -> Result<String> {
  id(api, &refs::user(arg, &api.instance())?).await
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let user = refs::user(arg, &api.instance())?;
  Ok(parse::user(&lookup(api, &user).await?, &api.instance()))
}

pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = user_id(api, arg).await?;
  let body = json!({"userId": id, "withRenotes": true, "withReplies": false});
  api.list("users/notes", body, page, parse::post).await
}

/// `users/followers` / `users/following`: relation rows paged by their own id.
pub async fn relations(
  api: &Api,
  arg: &str,
  followers: bool,
  page: &PageReq,
) -> Result<Page<User>> {
  let id = user_id(api, arg).await?;
  let (endpoint, key) = if followers {
    ("users/followers", "follower")
  } else {
    ("users/following", "followee")
  };
  let body = json!({"userId": id});
  api
    .list(endpoint, body, page, |row, instance| {
      parse::user(row.at(key), instance)
    })
    .await
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
  api.require()?;
  let mut body = json!({"markAsRead": false});
  if let Some(kind) = kind {
    body["includeTypes"] = json!([kind]);
  }
  api
    .list("i/notifications", body, page, parse::notification)
    .await
}

/// What `i` says is waiting: notifications, mentions, chat messages ...
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  let v = me(api).await?;
  let mut counts = BTreeMap::new();
  if let Some(count) = v.u64("unreadNotificationsCount") {
    counts.insert("notifications".into(), count);
  }
  for (key, field) in [
    ("notifications", "hasUnreadNotification"),
    ("mentions", "hasUnreadMentions"),
    ("specified_notes", "hasUnreadSpecifiedNotes"),
    ("chat", "hasUnreadChatMessages"),
    ("announcements", "hasUnreadAnnouncement"),
    ("antennas", "hasUnreadAntenna"),
  ] {
    if let Some(unread) = v.bool(field) {
      counts.entry(key.into()).or_insert(u64::from(unread));
    }
  }
  Ok(counts)
}
