//! Users: profiles, name lookup, followers / following and (un)follow.

use atp_core::text::from_secs;
use atp_core::{Action, Error, Page, PageReq, Query, Result, User, ValueExt, json};

use crate::api::Api;
use crate::paging::Pn;
use crate::refs::{self, UserRef};
use crate::{parse, videos};

const NAV: &str = "https://api.bilibili.com/x/web-interface/nav";

const CARD: &str = "https://api.bilibili.com/x/web-interface/card";
const FOLLOWERS: &str = "https://api.bilibili.com/x/relation/followers";
const FOLLOWINGS: &str = "https://api.bilibili.com/x/relation/followings";
const MODIFY: &str = "https://api.bilibili.com/x/relation/modify";
const UPSTAT: &str = "https://api.bilibili.com/x/space/upstat";

/// A user argument as a mid; names are looked up with user search (best match).
pub async fn mid(api: &Api, input: &str) -> Result<String> {
  let name = match refs::user(input) {
    UserRef::Mid(mid) => return Ok(mid.to_string()),
    UserRef::Name(name) => name,
  };
  let query = Query {
    keyword: name.clone(),
    ..Query::default()
  };
  let first = PageReq {
    cursor: None,
    size: 20,
  };
  let found = videos::search_users(api, &query, &first).await?;
  let best = found
    .items
    .iter()
    .find(|u| u.name.eq_ignore_ascii_case(&name))
    .or(found.items.first());
  best
    .map(|u| u.id.clone())
    .ok_or_else(|| Error::not_found(format!("no user named {name}")))
}

pub async fn profile(api: &Api, input: &str) -> Result<User> {
  let mid = mid(api, input).await?;
  let data = api
    .get(CARD)
    .arg("mid", &mid)
    .arg("photo", "false")
    .send()
    .await?;
  let mut user = parse::user(data.at("card"));
  user.stats.followers = data.count("follower").or(user.stats.followers);
  user.stats.posts = data.count("archive_count");
  if let Some(n) = data.count("article_count") {
    user.stats.other.insert("articles".into(), n);
  }
  user.stats.likes = data.count("like_num");
  if api.ctx.http.has_cookie("SESSDATA") {
    user.followed = data.bool("following");
  }
  // Lifetime plays of the uploads and reads of the articles.
  if let Ok(up) = api.get(UPSTAT).arg("mid", &mid).send().await {
    for (key, path) in [("views", "archive.view"), ("article_views", "article.view")] {
      if let Some(n) = up.count(path).filter(|n| *n > 0) {
        user.stats.other.insert(key.into(), n);
      }
    }
  }
  user.raw = Some(data);
  Ok(user)
}

/// Relation lists; Bilibili answers -101 / -352 to anonymous visitors.
async fn relations(api: &Api, url: &str, input: &str, page: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let mid = mid(api, input).await?;
  let pn = Pn::of(page, 50);
  let data = api
    .get(url)
    .arg("vmid", mid)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .arg("order", "desc")
    .send()
    .await?;
  let users = data
    .list("list")
    .iter()
    .map(|v| {
      let mut u = parse::user(v);
      // When the relation started (following / being followed).
      if let Some(at) = v.i64("mtime").and_then(from_secs) {
        u.extra.insert("followed_at".into(), json!(at));
      }
      u
    })
    .collect();
  Ok(pn.page(users, pn.before(data.u64("total"))))
}

/// Followers (others' lists are capped by Bilibili at 5 pages).
pub async fn followers(api: &Api, input: &str, page: &PageReq) -> Result<Page<User>> {
  relations(api, FOLLOWERS, input, page).await
}

pub async fn following(api: &Api, input: &str, page: &PageReq) -> Result<Page<User>> {
  relations(api, FOLLOWINGS, input, page).await
}

pub async fn follow(api: &Api, input: &str, undo: bool) -> Result<Action> {
  let mid = mid(api, input).await?;
  api
    .post(MODIFY)
    .arg("fid", &mid)
    .arg("act", if undo { 2 } else { 1 })
    .arg("re_src", 11)
    .send()
    .await?;
  let action = if undo { "unfollow" } else { "follow" };
  Ok(Action::done(action, &mid).with_url(refs::space_url(&mid)))
}

/// The logged-in account (from `nav`, which also refreshes the WBI keys).
pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let nav = api.get(NAV).send_raw().await?;
  api.remember_wbi(&nav);
  let data = nav.at("data");
  if nav.i64("code") != Some(0) || data.bool("isLogin") != Some(true) {
    return Err(Error::auth(
      "the session is not logged in (nav returned -101)",
    ));
  }
  let mut user = parse::user(data);
  if let Some(coins) = data.count("money") {
    user.stats.other.insert("coins".into(), coins);
  }
  Ok(user)
}

/// The logged-in account's mid: from the `DedeUserID` cookie, else from `nav`.
pub async fn my_mid(api: &Api) -> Result<String> {
  api.require_login()?;
  match api.ctx.http.cookie("DedeUserID") {
    Some(mid) => Ok(mid),
    None => Ok(whoami(api).await?.id),
  }
}
