//! Accounts: you, profiles, follows, and the playlists someone made or
//! collected.

use atp_core::{Action, Error, Page, PageReq, Post, Result, User, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

/// Session extra: the logged-in account's id.
const UID: &str = "uid";

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let v = api.web("w/nuser/account/get").send().await?;
  let user = parse::user(v.at("profile")).ok_or_else(|| {
    Error::auth("NetEase does not accept this session any more").with_hint(api.ctx.login_hint())
  })?;
  api.ctx.set_extra(UID, &user.id);
  Ok(user)
}

/// The logged-in account's id (asked once, then kept with the session).
pub async fn me(api: &Api) -> Result<String> {
  api.require_login()?;
  match api.ctx.extra(UID) {
    Some(uid) => Ok(uid),
    None => Ok(whoami(api).await?.id),
  }
}

/// A user id from an id, a profile link or an exact nickname.
pub async fn id(api: &Api, arg: &str) -> Result<String> {
  if let Some(id) = refs::user_id(arg) {
    return Ok(id);
  }
  let name = arg.trim().trim_start_matches('@');
  let v = api
    .client("cloudsearch/pc")
    .arg("s", name)
    .arg("type", 1002)
    .arg("limit", 10)
    .arg("offset", 0)
    .send()
    .await?;
  v.list("result.userprofiles")
    .iter()
    .find(|u| u.str("nickname").as_deref() == Some(name))
    .and_then(|u| u.str("userId"))
    .ok_or_else(|| Error::not_found(format!("no user named {name}")))
}

/// The user, or you without an argument.
pub async fn id_or_me(api: &Api, arg: Option<&str>) -> Result<String> {
  match arg {
    Some(arg) => id(api, arg).await,
    None => me(api).await,
  }
}

pub async fn profile(api: &Api, arg: &str) -> Result<User> {
  let uid = id(api, arg).await?;
  let v = api.web(&format!("v1/user/detail/{uid}")).send().await?;
  parse::profile(&v).ok_or_else(|| Error::not_found(format!("no user {uid}")))
}

pub async fn followers(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let uid = id(api, arg).await?;
  let offset = page.number_or(0);
  let limit = page.size_within(100) as u64;
  let v = api
    .client(&format!("user/getfolloweds/{uid}"))
    .arg("userId", uid.as_str())
    .arg("time", "0")
    .arg("limit", limit)
    .arg("offset", offset)
    .arg("getcounts", "true")
    .send()
    .await?;
  Ok(users_page(v.list("followeds"), offset, v.bool("more")))
}

pub async fn following(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let uid = id(api, arg).await?;
  let offset = page.number_or(0);
  let v = api
    .web(&format!("user/getfollows/{uid}"))
    .arg("offset", offset)
    .arg("limit", page.size_within(100) as u64)
    .arg("order", true)
    .send()
    .await?;
  Ok(users_page(v.list("follow"), offset, v.bool("more")))
}

fn users_page(list: &[atp_core::Value], offset: u64, more: Option<bool>) -> Page<User> {
  let users: Vec<User> = list.iter().filter_map(parse::user).collect();
  let end = offset + list.len() as u64;
  let next = (more == Some(true) && !list.is_empty()).then(|| end.to_string());
  Page::new(users, next)
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let uid = id(api, arg).await?;
  let path = if undo {
    "user/delfollow"
  } else {
    "user/follow"
  };
  api.web(&format!("{path}/{uid}")).write().send().await?;
  let action = if undo { "unfollow" } else { "follow" };
  Ok(Action::done(action, uid))
}

/// The playlists a user made (`collected` false) or collected (true).
pub async fn playlists(
  api: &Api,
  uid: &str,
  collected: bool,
  page: &PageReq,
) -> Result<Page<Post>> {
  let offset = page.number_or(0);
  let v = api
    .web("user/playlist")
    .arg("uid", uid)
    .arg("limit", page.size_within(100) as u64)
    .arg("offset", offset)
    .arg("includeVideo", true)
    .send()
    .await?;
  let list = v.list("playlist");
  let posts = list
    .iter()
    .filter(|p| (p.str("creator.userId").as_deref() != Some(uid)) == collected)
    .filter_map(parse::playlist)
    .collect();
  let end = offset + list.len() as u64;
  let next = (v.bool("more") == Some(true) && !list.is_empty()).then(|| end.to_string());
  Ok(Page::new(posts, next))
}

/// The daily check-in (每日签到) of the web and desktop players.
pub async fn signin(api: &Api) -> Result<Action> {
  let v = api
    .web("point/dailyTask")
    .arg("type", 1)
    .write()
    .send_raw()
    .await?;
  match v.i64("code") {
    Some(200) => {
      let points = v.u64("point").unwrap_or(0);
      Ok(Action::done("signin", "daily").with_message(format!("+{points} points")))
    }
    // 重复签到
    Some(-2) => Ok(Action::done("signin", "daily").with_message("already signed in today")),
    code => Err(api.error(code.unwrap_or(-1), &v)),
  }
}
