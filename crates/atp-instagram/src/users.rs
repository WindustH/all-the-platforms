//! Profiles, who follows whom, likers of a post, following, and the
//! logged-in account.

use atp_core::{Action, Error, Page, PageReq, Result, User, ValueExt};

use crate::api::{Api, with_cursor};
use crate::parse;
use crate::refs;

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  Ok(parse::user(&api.profile(arg).await?))
}

/// The logged-in account.
pub async fn whoami(api: &Api) -> Result<User> {
  api.require_session("whoami")?;
  let id = api
    .my_id()
    .ok_or_else(|| Error::auth("the session has no ds_user_id cookie"))?;
  let v = api.get(&format!("users/{id}/info/"), &[]).await?;
  Ok(parse::user(v.at("user")))
}

/// Followers (`edge` = `followers`) or following of a user.
pub async fn follows(api: &Api, arg: &str, edge: &str, req: &PageReq) -> Result<Page<User>> {
  api.require_session("follower lists")?;
  let id = api.user_id(arg).await?;
  let query = with_cursor(
    vec![("count", req.size_within(50).to_string())],
    "max_id",
    req.cursor.as_ref(),
  );
  let v = api
    .get(&format!("friendships/{id}/{edge}/"), &query)
    .await?;
  let users = v.list("users").iter().map(parse::user).collect();
  Ok(Page::new(users, v.str("next_max_id")))
}

pub async fn likers(api: &Api, post: &str) -> Result<Page<User>> {
  api.require_session("likers")?;
  let id = refs::media_id(post)?;
  let v = api.get(&format!("media/{id}/likers/"), &[]).await?;
  Ok(Page::last(
    v.list("users").iter().map(parse::user).collect(),
  ))
}

pub async fn follow(api: &Api, user: &str, undo: bool) -> Result<Action> {
  api.require_session("following")?;
  let id = api.user_id(user).await?;
  let verb = if undo { "destroy" } else { "create" };
  let form = vec![
    ("user_id", id.clone()),
    ("container_module", "profile".to_owned()),
  ];
  api
    .post(&format!("friendships/{verb}/{id}/"), &form)
    .await?;
  Ok(Action::done(if undo { "unfollow" } else { "follow" }, user).with_id(id))
}
