//! Accounts: yours, profiles, their statuses, followers, follows, and
//! notifications.

use atp_core::http::Method;
use atp_core::{Action, Error, Notification, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::{Api, Query};
use crate::parse;
use crate::refs::{self, AccountRef};

pub const NOTIFICATION_KINDS: &[&str] = &[
  "mention",
  "status",
  "reblog",
  "follow",
  "follow_request",
  "favourite",
  "poll",
  "update",
  "quote",
];

const ID_KEY: &str = "account_id";

pub async fn me(api: &Api) -> Result<Value> {
  api.require()?;
  let v = api
    .get("v1/accounts/verify_credentials", Vec::new())
    .await?;
  if let Some(id) = v.str("id") {
    api.ctx.set_extra(ID_KEY, &id);
  }
  Ok(v)
}

/// Your account's id, from the session or `verify_credentials`.
pub async fn my_id(api: &Api) -> Result<String> {
  if let Some(id) = api.ctx.extra(ID_KEY).filter(|id| !id.is_empty()) {
    return Ok(id);
  }
  Ok(me(api).await?.str("id").unwrap_or_default())
}

pub async fn whoami(api: &Api) -> Result<User> {
  Ok(parse::user(&me(api).await?, &api.instance()))
}

/// The account a reference points at, as Mastodon sends it. Accounts of
/// other servers the instance has not seen are looked up with `resolve`
/// (needs an account).
pub async fn lookup(api: &Api, arg: &str) -> Result<Value> {
  match refs::account(arg, &api.instance())? {
    AccountRef::Me => me(api).await,
    AccountRef::Id(id) => api.get(&format!("v1/accounts/{id}"), Vec::new()).await,
    AccountRef::Acct(acct) => {
      let found = api
        .get("v1/accounts/lookup", vec![("acct", acct.clone())])
        .await;
      match found {
        Err(e) if acct.contains('@') && api.logged_in() => resolve(api, &acct).await.map_err(|_| e),
        other => other,
      }
    }
  }
}

async fn resolve(api: &Api, acct: &str) -> Result<Value> {
  let query = vec![
    ("q", format!("@{acct}")),
    ("type", "accounts".to_owned()),
    ("resolve", "true".to_owned()),
  ];
  let v = api.get("v2/search", query).await?;
  let found = v.at("accounts.0");
  if !found.is_object() {
    return Err(Error::not_found(format!("no account @{acct}")));
  }
  Ok(found.clone())
}

pub async fn id(api: &Api, arg: &str) -> Result<String> {
  match refs::account(arg, &api.instance())? {
    AccountRef::Id(id) => Ok(id),
    AccountRef::Me => my_id(api).await,
    AccountRef::Acct(_) => Ok(lookup(api, arg).await?.str("id").unwrap_or_default()),
  }
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  Ok(parse::user(&lookup(api, arg).await?, &api.instance()))
}

/// An account's statuses, without replies; boosts included.
pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = id(api, arg).await?;
  let query: Query = vec![("exclude_replies", "true".into())];
  let path = format!("v1/accounts/{id}/statuses");
  api.list(&path, query, page, parse::post).await
}

/// `followers` / `following`.
pub async fn relations(
  api: &Api,
  arg: &str,
  followers: bool,
  page: &PageReq,
) -> Result<Page<User>> {
  let id = id(api, arg).await?;
  let edge = if followers { "followers" } else { "following" };
  let path = format!("v1/accounts/{id}/{edge}");
  api.list(&path, Vec::new(), page, parse::user).await
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = id(api, arg).await?;
  let verb = if undo { "unfollow" } else { "follow" };
  let v = api
    .write(
      Method::POST,
      &format!("v1/accounts/{id}/{verb}"),
      Vec::new(),
    )
    .await?;
  let mut done = Action::done(verb, &id);
  if v.bool("requested") == Some(true) {
    done = done.with_message("follow requested; the account approves followers");
  }
  Ok(done)
}

pub async fn notifications(
  api: &Api,
  kind: Option<&str>,
  page: &PageReq,
) -> Result<Page<Notification>> {
  api.require()?;
  let mut query: Query = Vec::new();
  if let Some(kind) = kind {
    query.push(("types[]", kind.to_owned()));
  }
  api
    .list("v1/notifications", query, page, parse::notification)
    .await
}
