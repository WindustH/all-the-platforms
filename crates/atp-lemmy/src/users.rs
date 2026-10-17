//! Users, your account, replies and mentions, unread counts, and the
//! instance itself.

use std::collections::BTreeMap;

use atp_core::{Error, Notification, Page, PageReq, Post, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs;

/// `notifications --type`.
pub const NOTIFICATION_KINDS: &[&str] = &["reply", "mention"];
const PAGE_MAX: usize = 50;

/// Your account as `site` gives it (`my_user.local_user_view`).
async fn me(api: &Api) -> Result<Value> {
  api.require()?;
  let v = api.get("site", Vec::new()).await?;
  let me = v.at("my_user.local_user_view");
  if me.is_null() {
    return Err(
      Error::auth(format!("{} did not accept the token", api.instance()))
        .with_hint("log in again: `atp guide login lemmy`"),
    );
  }
  Ok(me.clone())
}

pub async fn whoami(api: &Api) -> Result<User> {
  let me = me(api).await?;
  let mut user = parse::person(me.at("person"));
  user.stats.posts = me.u64("counts.post_count");
  Ok(user)
}

/// Your person id on the instance.
pub async fn my_id(api: &Api) -> Result<u64> {
  me(api)
    .await?
    .u64("person.id")
    .ok_or_else(|| Error::upstream("Lemmy: `site` named no person for the token"))
}

/// `(page, limit)` of a numbered listing.
fn numbered(page: &PageReq) -> (u64, usize) {
  (page.number_or(1), page.size_within(PAGE_MAX))
}

fn next(rows: usize, number: u64, limit: usize) -> Option<String> {
  (rows >= limit).then(|| (number + 1).to_string())
}

async fn person(api: &Api, arg: &str, params: Vec<(&str, String)>) -> Result<Value> {
  let name = refs::user(arg, &api.instance())?;
  let mut all = vec![("username", name)];
  all.extend(params);
  api.get("user", all).await
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let v = person(api, arg, vec![("limit", "1".to_owned())]).await?;
  Ok(parse::person_view(v.at("person_view")))
}

/// A user's posts, the newest first.
pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let (number, limit) = numbered(page);
  let params = vec![
    ("sort", "New".to_owned()),
    ("page", number.to_string()),
    ("limit", limit.to_string()),
  ];
  let v = person(api, arg, params).await?;
  let instance = api.instance();
  let rows = v.list("posts");
  Ok(Page::new(
    rows.iter().map(|row| parse::post(row, &instance)).collect(),
    next(rows.len(), number, limit),
  ))
}

/// Replies to your posts and comments, or mentions of you.
pub async fn notifications(
  api: &Api,
  kind: Option<&str>,
  page: &PageReq,
) -> Result<Page<Notification>> {
  api.require()?;
  let (endpoint, list, record, kind) = match kind.unwrap_or("reply") {
    "mention" => ("user/mention", "mentions", "person_mention", "mention"),
    _ => ("user/replies", "replies", "comment_reply", "reply"),
  };
  let (number, limit) = numbered(page);
  let params = vec![
    ("sort", "New".to_owned()),
    ("page", number.to_string()),
    ("limit", limit.to_string()),
  ];
  let v = api.get(endpoint, params).await?;
  let instance = api.instance();
  let rows = v.list(list);
  Ok(Page::new(
    rows
      .iter()
      .map(|row| parse::notification(row, kind, record, &instance))
      .collect(),
    next(rows.len(), number, limit),
  ))
}

pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  api.require()?;
  let v = api.get("user/unread_count", Vec::new()).await?;
  Ok(
    ["replies", "mentions", "private_messages"]
      .into_iter()
      .map(|key| (key.to_owned(), v.u64(key).unwrap_or(0)))
      .collect(),
  )
}

/// The instance: name, version, description and counts.
pub async fn instance(api: &Api) -> Result<Value> {
  let v = api.get("site", Vec::new()).await?;
  let site = v.at("site_view.site");
  let counts = v.at("site_view.counts");
  Ok(json!({
    "instance": api.instance(),
    "name": site.str("name"),
    "description": site.str("description"),
    "version": v.str("version"),
    "users": counts.u64("users"),
    "posts": counts.u64("posts"),
    "comments": counts.u64("comments"),
    "communities": counts.u64("communities"),
    "active_month": counts.u64("users_active_month"),
    "registration": v.str("site_view.local_site.registration_mode"),
    "downvotes": v.bool("site_view.local_site.enable_downvotes"),
  }))
}
