//! Users: profiles (`/1.0/users/profile`), what they posted
//! (`/1.0/personalUpdate/single`), followers and following
//! (`/1.0/userRelation/…`), and following someone. Without a login a
//! profile and its latest posts come from the user's page on m.okjike.com.

use atp_core::{Action, Error, Page, PageReq, Post, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::{parse, refs};

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let v = api.get("/1.0/users/profile", &[]).await?;
  profile(&v)
}

fn profile(v: &Value) -> Result<User> {
  let user = v.at("user");
  if user.str("username").is_none() {
    return Err(Error::upstream("Jike answered no profile"));
  }
  Ok(parse::user(user))
}

/// The m.okjike.com page of a user: `user` and their latest `posts`.
async fn page(api: &Api, username: &str) -> Result<Value> {
  let props = api.page(&format!("/users/{username}")).await?;
  if props.str("user.username").is_none() {
    return Err(Error::not_found(format!("Jike has no user {username}")));
  }
  Ok(props)
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let username = refs::user(arg)?;
  if !api.logged_in() {
    return Ok(parse::user(page(api, &username).await?.at("user")));
  }
  let v = api
    .get("/1.0/users/profile", &[("username", username)])
    .await?;
  profile(&v)
}

/// What a user posted and reposted, the newest first. Without a login,
/// the latest few their page shows.
pub async fn posts(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let username = refs::user(arg)?;
  if !api.logged_in() {
    let props = page(api, &username).await?;
    let items = props.list("posts").iter().map(parse::post).collect();
    return Ok(Page::last(items));
  }
  let body = json!({"username": username});
  let (v, next) = api.list("/1.0/personalUpdate/single", body, req).await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::post).collect(),
    next,
  ))
}

async fn relations(api: &Api, path: &str, arg: &str, req: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let body = json!({"username": refs::user(arg)?});
  let (v, next) = api.list(path, body, req).await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::user).collect(),
    next,
  ))
}

pub async fn followers(api: &Api, arg: &str, req: &PageReq) -> Result<Page<User>> {
  relations(api, "/1.0/userRelation/getFollowerList", arg, req).await
}

pub async fn following(api: &Api, arg: &str, req: &PageReq) -> Result<Page<User>> {
  relations(api, "/1.0/userRelation/getFollowingList", arg, req).await
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let username = refs::user(arg)?;
  let verb = if undo { "unfollow" } else { "follow" };
  api
    .write(
      &format!("/1.0/userRelation/{verb}"),
      json!({"username": username}),
    )
    .await?;
  Ok(Action::done(verb, username))
}
