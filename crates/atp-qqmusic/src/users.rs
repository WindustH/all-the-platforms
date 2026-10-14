//! Accounts: you, profiles, the playlists someone made, follows. Users go
//! by the encrypted id QQ Music shows (`encrypt_uin`); account numbers and
//! `@name` are turned into one first.

use atp_core::{Action, Error, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::refs::{self, UserRef};
use crate::{parse, singers};

/// Session extra: the logged-in account's encrypted id.
pub const EUIN: &str = "euin";
/// The profile page all profiles come from.
const PROFILE: &str = "/rsc/fcgi-bin/fcg_get_profile_homepage.fcg";
/// The page of the playlists someone made.
const CREATED: &str = "/rsc/fcgi-bin/fcg_user_created_diss";
/// Playlists one page of `user-posts` fetches at most.
const PAGE_PLAYLISTS: usize = 50;
/// Users one page of a follow list fetches at most.
const PAGE_USERS: usize = 50;

/// The profile page of `userid` (0: the session's own account).
async fn profile_page(api: &Api, userid: &str) -> Result<Value> {
  api
    .page(
      PROFILE,
      &[
        ("cid", "205360838".to_owned()),
        ("ct", "24".to_owned()),
        ("userid", userid.to_owned()),
        ("reqfrom", "1".to_owned()),
        ("reqtype", "0".to_owned()),
      ],
    )
    .await
}

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let page = profile_page(api, "0").await?;
  let user = parse::user(page.at("data.creator")).ok_or_else(|| {
    Error::auth("QQ Music does not accept this session any more").with_hint(api.ctx.login_hint())
  })?;
  api.ctx.set_extra(EUIN, &user.id);
  Ok(user)
}

/// The logged-in account's encrypted id (asked once, then kept with the
/// session).
pub async fn me(api: &Api) -> Result<String> {
  api.require_login()?;
  match api.ctx.extra(EUIN) {
    Some(euin) => Ok(euin),
    None => Ok(whoami(api).await?.id),
  }
}

/// The encrypted id of a user argument.
pub async fn euin(api: &Api, arg: &str) -> Result<String> {
  match refs::user(arg)? {
    UserRef::Euin(euin) => Ok(euin),
    UserRef::Uin(uin) => created_page(api, &uin.to_string(), 0, 1)
      .await?
      .str("data.encrypt_uin")
      .ok_or_else(|| Error::not_found(format!("no user {uin}"))),
    UserRef::Name(name) => by_name(api, &name).await,
  }
}

/// The user, or you without an argument.
pub async fn euin_or_me(api: &Api, arg: Option<&str>) -> Result<String> {
  match arg {
    Some(arg) => euin(api, arg).await,
    None => me(api).await,
  }
}

/// A user with exactly this nickname, by search.
async fn by_name(api: &Api, name: &str) -> Result<String> {
  let query = atp_core::Query {
    keyword: name.to_owned(),
    ..atp_core::Query::default()
  };
  let req = PageReq {
    cursor: None,
    size: 20,
  };
  crate::search::users(api, &query, &req)
    .await?
    .items
    .into_iter()
    .find(|u| u.name == name)
    .map(|u| u.id)
    .ok_or_else(|| Error::not_found(format!("no user named {name}")))
}

/// A profile. QQ Music shows profiles to logged-in visitors; without a
/// login only the nickname and the number of playlists are known.
pub async fn profile(api: &Api, arg: &str) -> Result<User> {
  let euin = euin(api, arg).await?;
  if api.require_login().is_ok() {
    let page = profile_page(api, &euin).await?;
    let mut user = parse::user(page.at("data.creator"))
      .ok_or_else(|| Error::not_found(format!("no user {euin}")))?;
    user.stats.posts = user.stats.posts.or_else(|| page.u64("data.mydiss.num"));
    user.raw = Some(page);
    return Ok(user);
  }
  let page = created_page(api, &euin, 0, 1).await?;
  parse::user(page.at("data")).ok_or_else(|| Error::not_found(format!("no user {euin}")))
}

/// The page of playlists `host` (an encrypted id or a number) made.
async fn created_page(api: &Api, host: &str, start: u64, size: usize) -> Result<Value> {
  api
    .page(
      CREATED,
      &[
        ("hostuin", host.to_owned()),
        ("sin", start.to_string()),
        ("size", size.to_string()),
      ],
    )
    .await
}

/// The playlists a user made (system ones without an id left out).
pub async fn playlists(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let euin = euin(api, arg).await?;
  let start = page.number_or(0);
  let data = created_page(api, &euin, start, page.size_within(PAGE_PLAYLISTS)).await?;
  let list = data.list("data.disslist");
  let owner = parse::user(data.at("data"));
  let posts = list
    .iter()
    .filter_map(parse::playlist)
    .map(|mut p| {
      p.author = owner.clone().map(|u| User { raw: None, ..u });
      p
    })
    .collect();
  let end = start + list.len() as u64;
  let total = data.u64("data.totoal").unwrap_or(0);
  let next = (!list.is_empty() && end < total).then(|| end.to_string());
  Ok(Page::new(posts, next))
}

/// A follow list (`GetFansList`, `GetFollowUserList`) of a user; needs a
/// login.
async fn relations(api: &Api, method: &str, arg: &str, page: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let euin = euin(api, arg).await?;
  let from = page.number_or(0);
  let data = api
    .call("music.concern.RelationList", method)
    .arg("HostUin", euin)
    .arg("From", from)
    .arg("Size", page.size_within(PAGE_USERS))
    .send()
    .await?;
  let list = data.list("List");
  let users = list.iter().filter_map(parse::user).collect();
  let end = from + list.len() as u64;
  let more = data.bool("HasMore").unwrap_or(false) && !list.is_empty();
  Ok(Page::new(users, more.then(|| end.to_string())))
}

pub async fn followers(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  relations(api, "GetFansList", arg, page).await
}

pub async fn following(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  relations(api, "GetFollowUserList", arg, page).await
}

/// Follow a singer (`singer:MID` or a singer link) or a user.
pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  if let Some(mid) = refs::singer(arg) {
    return singers::follow(api, singers::SINGER, &mid, undo).await;
  }
  let euin = euin(api, arg).await?;
  singers::follow(api, singers::USER, &euin, undo).await
}
