//! Users (per site), their posts, and the inbox of the logged-in account.

use atp_core::{
  Error, Notification, Page, PageReq, Post, Result, User, ValueExt, credentials, json,
};

use crate::api::{Api, FILTER_LIST, Params};
use crate::parse;
use crate::refs::{self, UserRef};

/// The account of the access token, or the app key itself when there is
/// only a key (its daily quota in `extra`).
pub async fn whoami(api: &Api) -> Result<User> {
  if api.token().is_some() {
    return user(api, "me").await;
  }
  let Some(key) = api.key() else {
    return Err(Error::auth("not logged in").with_hint(api.ctx.login_hint()));
  };
  let site = api.site();
  let v = api.get("info", Some(&site), Vec::new(), "default").await?;
  let mut user = User {
    id: credentials::mask(&key),
    name: "Stack Exchange app key".into(),
    url: Some("https://stackapps.com/apps/oauth".into()),
    ..User::default()
  };
  for (name, count) in parse::quota(&v) {
    user.extra.insert(format!("quota_{name}"), json!(count));
  }
  Ok(user)
}

/// `(method path, site)` of a user: `users/<id>` or `me`.
fn path(api: &Api, user: &UserRef) -> Result<(String, String)> {
  let site = api.site_or(&user.site);
  match &user.id {
    Some(id) => Ok((format!("users/{id}"), site)),
    None => {
      api.require()?;
      Ok(("me".into(), site))
    }
  }
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let (method, site) = path(api, &refs::user(arg)?)?;
  let v = api
    .get(&method, Some(&site), Vec::new(), FILTER_LIST)
    .await?;
  v.list("items")
    .first()
    .map(parse::user)
    .ok_or_else(|| Error::not_found(format!("no user {arg} on {site}")))
}

/// Questions and answers of a user, newest first.
pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let (method, site) = path(api, &refs::user(arg)?)?;
  let params: Params = vec![("sort", "creation".into()), ("order", "desc".into())];
  api
    .list(
      &format!("{method}/posts"),
      Some(&site),
      params,
      FILTER_LIST,
      page,
      parse::post,
    )
    .await
}

/// The inbox across every site (needs the `read_inbox` scope).
pub async fn notifications(api: &Api, page: &PageReq) -> Result<Page<Notification>> {
  api.require()?;
  api
    .list(
      "inbox",
      None,
      Vec::new(),
      FILTER_LIST,
      page,
      parse::notification,
    )
    .await
}
