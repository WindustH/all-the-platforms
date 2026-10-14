//! Accounts: yours, profiles, their posts, followers and follows, and
//! notifications.

use std::collections::BTreeMap;

use atp_core::{Notification, Page, PageReq, Post, Result, User, ValueExt};

use crate::api::{Api, Query};
use crate::parse;
use crate::refs::{self, ActorRef};

pub const NOTIFICATION_KINDS: &[&str] = &[
  "like",
  "repost",
  "follow",
  "mention",
  "reply",
  "quote",
  "starterpack-joined",
];

/// A user argument as the API takes actors (a handle or a DID).
pub fn actor(api: &Api, arg: &str) -> Result<String> {
  match refs::actor(arg)? {
    ActorRef::Me => api.did(),
    ActorRef::Actor(actor) => Ok(actor),
  }
}

pub async fn whoami(api: &Api) -> Result<User> {
  let did = api.did()?;
  let v = api
    .get_auth("app.bsky.actor.getProfile", vec![("actor", did)])
    .await?;
  Ok(parse::user(&v))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let query = vec![("actor", actor(api, arg)?)];
  let v = api.get("app.bsky.actor.getProfile", query).await?;
  Ok(parse::user(&v))
}

/// An account's posts and reposts, without its replies.
pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let query: Query = vec![
    ("actor", actor(api, arg)?),
    ("filter", "posts_no_replies".to_owned()),
  ];
  api
    .list(
      "app.bsky.feed.getAuthorFeed",
      query,
      "feed",
      page,
      parse::feed_item,
    )
    .await
}

/// `followers` / `following`.
pub async fn relations(
  api: &Api,
  arg: &str,
  followers: bool,
  page: &PageReq,
) -> Result<Page<User>> {
  let (nsid, key) = if followers {
    ("app.bsky.graph.getFollowers", "followers")
  } else {
    ("app.bsky.graph.getFollows", "follows")
  };
  let query = vec![("actor", actor(api, arg)?)];
  api
    .list(nsid, query, key, page, |v| Some(parse::user(v)))
    .await
}

pub async fn notifications(
  api: &Api,
  kind: Option<&str>,
  page: &PageReq,
) -> Result<Page<Notification>> {
  api.did()?;
  let mut query: Query = Vec::new();
  if let Some(kind) = kind {
    query.push(("reasons", kind.to_owned()));
  }
  api
    .list(
      "app.bsky.notification.listNotifications",
      query,
      "notifications",
      page,
      |v| Some(parse::notification(v)),
    )
    .await
}

pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  let v = api
    .get_auth("app.bsky.notification.getUnreadCount", Vec::new())
    .await?;
  Ok(BTreeMap::from([(
    "notifications".to_owned(),
    v.u64("count").unwrap_or(0),
  )]))
}
