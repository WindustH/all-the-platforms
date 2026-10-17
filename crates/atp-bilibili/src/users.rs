//! Users: profiles, name lookup, and the followers / following lists
//! (sorted, searched, mutual).

use atp_core::{Error, FollowQuery, Page, PageReq, Query, Result, User, ValueExt, paging};

use crate::api::{Api, Call};
use crate::paging::Pn;
use crate::parse::{self, Rows};
use crate::refs::{self, UserRef};
use crate::videos;

const NAV: &str = "https://api.bilibili.com/x/web-interface/nav";

const CARD: &str = "https://api.bilibili.com/x/web-interface/card";
const FOLLOWERS: &str = "https://api.bilibili.com/x/relation/followers";
const FOLLOWINGS: &str = "https://api.bilibili.com/x/relation/followings";
const SEARCH: &str = "https://api.bilibili.com/x/relation/followings/search";
const FRIENDS: &str = "https://api.bilibili.com/x/relation/friends";
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

/// One page of a relation list; Bilibili answers -101 / -352 to
/// anonymous visitors.
async fn rows(call: Call<'_>, rows: Rows, page: &PageReq) -> Result<Page<User>> {
  let pn = Pn::of(page, 50);
  let data = call.arg("pn", pn.pn).arg("ps", pn.ps).send().await?;
  let users = data
    .list("list")
    .iter()
    .map(|v| parse::relation(v, rows))
    .collect();
  Ok(pn.page(users, pn.before(data.u64("total"))))
}

/// A user argument as a mid, and whether it is the logged-in account.
async fn whose(api: &Api, input: &str) -> Result<(String, bool)> {
  api.require_login()?;
  let mid = mid(api, input).await?;
  let own = my_mid(api).await? == mid;
  Ok((mid, own))
}

/// Followers (others' lists are capped by Bilibili at 5 pages).
pub async fn followers(api: &Api, input: &str, page: &PageReq) -> Result<Page<User>> {
  let (mid, own) = whose(api, input).await?;
  let call = api.get(FOLLOWERS).arg("vmid", &mid).arg("order", "desc");
  let kind = if own { Rows::MyFollowers } else { Rows::Others };
  rows(call, kind, page).await
}

/// Whom a user follows, in follow order or (your own) most visited
/// first, searched by name, or your mutual follows.
pub async fn following(
  api: &Api,
  input: &str,
  query: &FollowQuery,
  page: &PageReq,
) -> Result<Page<User>> {
  let (mid, own) = whose(api, input).await?;
  let kind = if own { Rows::MyFollowing } else { Rows::Others };
  if query.mutual {
    if !own || query.search.is_some() || query.sort.is_some() {
      return Err(Error::input(
        "--mutual lists your own mutual follows, without USER, --sort or --search",
      ));
    }
    let data = api.get(FRIENDS).send().await?;
    let users = data.list("list").iter().map(|v| parse::relation(v, kind));
    return Ok(paging::slice(users.collect(), page));
  }
  let call = match (&query.search, query.sort.as_deref()) {
    (Some(_), Some(_)) => {
      return Err(Error::input(
        "--search lists matches in follow order; drop --sort",
      ));
    }
    (Some(words), None) => api.get(SEARCH).arg("vmid", &mid).arg("name", words),
    (None, None | Some("recent")) => api.get(FOLLOWINGS).arg("vmid", &mid).arg("order", "desc"),
    (None, Some("visited")) if own => api
      .get(FOLLOWINGS)
      .arg("vmid", &mid)
      .arg("order_type", "attention"),
    (None, Some("visited")) => {
      return Err(Error::input(
        "--sort visited orders only your own following",
      ));
    }
    (None, Some(sort)) => {
      return Err(Error::input(format!(
        "unknown --sort {sort} (recent or visited)"
      )));
    }
  };
  let call = call
    .arg("gaia_source", "main_web")
    .arg("web_location", "333.1387");
  rows(call, kind, page).await
}

/// User arguments as mids, all of them before anything is sent.
pub async fn mids(api: &Api, inputs: &[String]) -> Result<Vec<String>> {
  let mut out = Vec::with_capacity(inputs.len());
  for input in inputs {
    out.push(mid(api, input).await?);
  }
  Ok(out)
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
