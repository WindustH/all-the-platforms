//! Likes and follows. Both endpoints come from 2022 references and carry
//! the secsdk CSRF token the web client fetches with a `HEAD` request: they
//! are unverified against today's site.

use std::time::Duration;

use atp_core::{Action, Error, Result};

use crate::api::{self, Api, WWW};
use crate::refs;

const LIKE: &str = "/api/pc/user/fans_digg";
const FOLLOW: &str = "/c/user/follow/";

pub async fn like(api: &Api, post: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let post = refs::post(api, post).await?;
  pause(api).await;
  let csrf = csrf_token(api, LIKE).await;
  let resp = api
    .ctx
    .http
    .post(format!("{WWW}{LIKE}"))
    .query("op", if undo { 0 } else { 1 })
    .query("gid", &post.id)
    .header("x-secsdk-csrf-token", csrf.unwrap_or_default())
    .send()
    .await?;
  api::check(resp.value()?)?;
  let name = if undo { "unlike" } else { "like" };
  Ok(Action::done(name, &post.id).with_url(refs::post_url(&post.id)))
}

/// Follow a user by numeric id (the follow API takes no token).
pub async fn follow(api: &Api, user: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  if undo {
    return Err(Error::unsupported("follow --undo"));
  }
  let user_id = user.trim();
  if !user_id.bytes().all(|b| b.is_ascii_digit()) || user_id.is_empty() {
    return Err(Error::input(
      "Toutiao follows by numeric user id (the `id` of a post's author)",
    ));
  }
  pause(api).await;
  let csrf = csrf_token(api, FOLLOW).await;
  let resp = api
    .ctx
    .http
    .post(format!("{WWW}{FOLLOW}"))
    .header("x-secsdk-csrf-token", csrf.unwrap_or_default())
    .form([("user_id", user_id)])
    .send()
    .await?;
  api::check(resp.value()?)?;
  Ok(Action::done("follow", user_id))
}

/// The secsdk CSRF token of `path` (`x-ware-csrf-token: 0,<token>,…`).
async fn csrf_token(api: &Api, path: &str) -> Option<String> {
  let resp = api
    .ctx
    .http
    .request(atp_core::http::Method::HEAD, format!("{WWW}{path}"))
    .header("x-secsdk-csrf-request", "1")
    .header("x-secsdk-csrf-version", "1.2.22")
    .send()
    .await
    .ok()?;
  let header = resp.header("x-ware-csrf-token")?;
  header
    .split(',')
    .nth(1)
    .map(str::to_owned)
    .filter(|t| !t.is_empty())
}

async fn pause(api: &Api) {
  api
    .ctx
    .http
    .pause(Duration::from_millis(800), Duration::from_millis(2000))
    .await;
}
