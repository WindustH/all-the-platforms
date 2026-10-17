//! Accounts: the logged-in user, profiles, people search, a user's comments
//! and blocking.

use atp_core::bulk;
use atp_core::text::from_secs;
use atp_core::{Action, Comment, Error, Page, PageReq, Query, Result, User, ValueExt};

use crate::api::Api;
use crate::{listings, parse, refs};

pub async fn whoami(api: &Api) -> Result<User> {
  let me = api.me().await?;
  parse::user(&me).ok_or_else(|| Error::auth("Reddit did not return the account"))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let name = refs::user(arg)?;
  let v = api.get(&format!("/user/{name}/about"), &[]).await?;
  let mut user = parse::data(&v, "t2")
    .and_then(parse::user)
    .ok_or_else(|| Error::not_found(format!("user u/{name} not found")))?;
  if !api.logged_in() {
    user.followed = None;
  }
  Ok(user)
}

pub async fn search(api: &Api, q: &Query, req: &PageReq) -> Result<Page<User>> {
  let query = vec![("q", q.keyword.clone())];
  listings::page(api, "/users/search", query, req, |t| {
    parse::data(t, "t2").and_then(parse::user)
  })
  .await
}

/// Comments a user wrote, newest first.
pub async fn comments(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Comment>> {
  let name = refs::user(arg)?;
  let query = vec![("sort", "new".into())];
  listings::page(api, &format!("/user/{name}/comments"), query, req, |t| {
    parse::comment(t, None)
  })
  .await
}

/// Block users, or unblock them with `undo`, one call each.
pub async fn block_all(api: &Api, users: &[String], undo: bool) -> Result<Vec<Action>> {
  let name = if undo { "unblock" } else { "block" };
  bulk::each(name, users, async |user| block(api, user, undo).await).await
}

/// Block a user, or unblock with `undo`.
async fn block(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let name = refs::user(arg)?;
  if undo {
    let me = api.me().await?;
    let container = format!("t2_{}", me.str("id").unwrap_or_default());
    let form = vec![
      ("container", container),
      ("name", name.clone()),
      ("type", "enemy".into()),
    ];
    api.post("/r/all/api/unfriend", form).await?;
    return Ok(Action::done("unblock", format!("u/{name}")));
  }
  api
    .post("/api/block_user", vec![("name", name.clone())])
    .await?;
  Ok(Action::done("block", format!("u/{name}")))
}

/// Accounts you block (`/prefs/blocked`), with since when in
/// `extra.blocked_at`; Reddit gives them all at once.
pub async fn blocked(api: &Api) -> Result<Page<User>> {
  api.require_login()?;
  let v = api.get("/prefs/blocked", &[]).await?;
  let users = v
    .list("data.children")
    .iter()
    .filter_map(|b| {
      let name = b.str("name")?;
      let mut user = parse::named(&name, b.str("id"));
      if let Some(t) = b.i64("date").and_then(from_secs) {
        user.extra.insert("blocked_at".into(), t.to_string().into());
      }
      Some(user)
    })
    .collect();
  Ok(Page::last(users))
}
