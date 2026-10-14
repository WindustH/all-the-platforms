//! Users: profiles, followers and followings, following them.

use atp_core::{Action, Page, PageReq, Result, User, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs;

const PAGE_MAX: usize = 20;
/// `type` of a user in follow calls.
const USER_TYPE: u64 = 1;

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let v = api
    .get("/user_api/v1/user/get", &[("not_self", "0".to_owned())])
    .await?;
  Ok(parse::user(v.at("data")))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = refs::user(arg)?;
  let params = [
    ("user_id", id),
    ("not_self", "1".to_owned()),
    ("need_badge", "1".to_owned()),
  ];
  let v = api.get("/user_api/v1/user/get", &params).await?;
  Ok(parse::user(v.at("data")))
}

/// Followers (`followers`) or followings (`followees`) of a user.
pub async fn follows(api: &Api, arg: &str, which: &str, page: &PageReq) -> Result<Page<User>> {
  let id = refs::user(arg)?;
  let params = [
    ("user_id", id),
    ("cursor", page.cursor.clone().unwrap_or_else(|| "0".into())),
    ("limit", page.size_within(PAGE_MAX).to_string()),
  ];
  let v = api
    .get(&format!("/user_api/v1/follow/{which}"), &params)
    .await?;
  let data = v.at("data");
  let rows = data.list("data");
  let items = rows.iter().map(parse::user).collect();
  let more = !rows.is_empty() && data.bool("hasMore").unwrap_or(false);
  Ok(Page::new(items, more.then(|| data.str("cursor")).flatten()))
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::user(arg)?;
  let (path, action) = if undo {
    ("/interact_api/v1/follow/undo", "unfollow")
  } else {
    ("/interact_api/v1/follow/do", "follow")
  };
  api
    .write(path, &json!({"id": id, "type": USER_TYPE}))
    .await?;
  Ok(Action::done(action, arg))
}

/// The logged-in user's id, for `--folder`-less favorites and own lists.
pub async fn my_id(api: &Api) -> Result<String> {
  Ok(whoami(api).await?.id)
}
