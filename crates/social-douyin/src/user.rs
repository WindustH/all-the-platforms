//! Users: profiles, the logged-in account, their posts and liked posts,
//! followers and following.

use social_core::{Ctx, Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::refs::{self, UserArg};
use crate::{api, discover, parse};

const PROFILE: &str = "/aweme/v1/web/user/profile/other/";
const SELF: &str = "/aweme/v1/web/user/profile/self/";
const POSTS: &str = "/aweme/v1/web/aweme/post/";
const LIKES: &str = "/aweme/v1/web/aweme/favorite/";
const FOLLOWERS: &str = "/aweme/v1/web/user/follower/list/";
const FOLLOWING: &str = "/aweme/v1/web/user/following/list/";

/// The `sec_uid` of a user reference; `@douyin_id` is looked up by search.
pub async fn sec_uid(ctx: &Ctx, arg: &str) -> Result<String> {
  match refs::user(ctx, arg).await? {
    UserArg::SecUid(s) => Ok(s),
    UserArg::Handle(handle) => {
      let q = Query {
        keyword: handle.clone(),
        ..Query::default()
      };
      let found = discover::search_users(
        ctx,
        &q,
        &PageReq {
          cursor: None,
          size: 10,
        },
      )
      .await?;
      found
        .items
        .into_iter()
        .find(|u| {
          u.handle.as_deref() == Some(handle.as_str())
            || u.extra.get("short_id").and_then(Value::as_str) == Some(handle.as_str())
        })
        .map(|u| u.id)
        .ok_or_else(|| Error::not_found(format!("no Douyin user with the id {handle}")))
    }
  }
}

async fn profile_raw(ctx: &Ctx, sec: &str) -> Result<Value> {
  let v = api::get(ctx, PROFILE)
    .arg("publish_video_strategy_type", "2")
    .arg("source", "channel_pc_web")
    .arg("sec_user_id", sec)
    .arg("personal_center_strategy", "1")
    .arg("profile_other_record_enable", "1")
    .arg("land_to", "1")
    .arg("version_code", "170400")
    .arg("version_name", "17.4.0")
    .send()
    .await?;
  match v.at("user") {
    u if u.is_object() => Ok(u.clone()),
    _ => Err(Error::not_found(format!("no Douyin user {sec}"))),
  }
}

pub async fn profile(ctx: &Ctx, arg: &str) -> Result<User> {
  let sec = sec_uid(ctx, arg).await?;
  let v = profile_raw(ctx, &sec).await?;
  parse::user(&v).ok_or_else(|| Error::upstream("Douyin returned a user without a sec_uid"))
}

pub async fn whoami(ctx: &Ctx) -> Result<User> {
  api::require_login(ctx)?;
  let v = api::get(ctx, SELF)
    .arg("publish_video_strategy_type", "2")
    .arg("source", "channel_pc_web")
    .arg("personal_center_strategy", "1")
    .send()
    .await?;
  parse::user(v.at("user")).ok_or_else(|| {
    Error::auth("Douyin did not return the logged-in account")
      .with_hint("run `social douyin login --browser`")
  })
}

/// A `max_cursor` listing of awemes (posts, likes).
async fn awemes(ctx: &Ctx, path: &str, sec: &str, page: &PageReq, own: bool) -> Result<Page<Post>> {
  let cursor = page.number_or(0);
  let mut call = api::get(ctx, path)
    .arg("sec_user_id", sec)
    .arg("max_cursor", cursor)
    .arg("min_cursor", "0")
    .arg("count", page.size_within(18))
    .arg("publish_video_strategy_type", "2")
    .arg("whale_cut_token", "")
    .arg("cut_version", "1")
    .arg("version_code", "170400")
    .arg("version_name", "17.4.0");
  if path == POSTS {
    call = call
      .arg("locate_query", "false")
      .arg("show_live_replay_strategy", "1")
      .arg("need_time_list", "1")
      .arg("time_list_query", "0")
      .arg("from_user_page", if own { "0" } else { "1" });
  }
  let v = call.send().await?;
  let items: Vec<Post> = v
    .list("aweme_list")
    .iter()
    .filter_map(parse::aweme)
    .collect();
  let next = (v.i64("has_more") == Some(1) && !items.is_empty())
    .then(|| v.str("max_cursor"))
    .flatten()
    .filter(|c| c != "0");
  Ok(Page::new(items, next))
}

pub async fn posts(ctx: &Ctx, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let sec = sec_uid(ctx, arg).await?;
  awemes(ctx, POSTS, &sec, page, false).await
}

/// Posts a user liked (public only when the user allows it).
pub async fn likes(ctx: &Ctx, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let sec = match user {
    Some(u) => sec_uid(ctx, u).await?,
    None => whoami(ctx).await?.id,
  };
  awemes(ctx, LIKES, &sec, page, user.is_none()).await
}

/// Followers or followings, paged by `max_time` (seconds; the first page
/// asks with the current time, `0` would return an empty list).
async fn relations(
  ctx: &Ctx,
  arg: &str,
  path: &str,
  key: &str,
  page: &PageReq,
) -> Result<Page<User>> {
  let sec = sec_uid(ctx, arg).await?;
  let uid = profile_raw(ctx, &sec)
    .await?
    .str("uid")
    .ok_or_else(|| Error::upstream("profile without uid"))?;
  let now = jiff::Timestamp::now().as_second().to_string();
  let max_time = page.cursor.clone().unwrap_or(now);
  let mut call = api::get(ctx, path)
    .arg("user_id", &uid)
    .arg("sec_user_id", &sec)
    .arg("offset", "0")
    .arg("min_time", "0")
    .arg("max_time", &max_time)
    .arg("count", page.size_within(20))
    .arg("source_type", "1")
    .arg("gps_access", "0")
    .arg("address_book_access", "0");
  if path == FOLLOWING {
    call = call.arg("is_top", "1");
  }
  let v = call.send().await?;
  let items: Vec<User> = v.list(key).iter().filter_map(parse::user).collect();
  let next = (v.i64("has_more") == Some(1) && !items.is_empty())
    .then(|| v.str("min_time"))
    .flatten()
    .filter(|t| *t != max_time && t != "0");
  Ok(Page::new(items, next))
}

pub async fn followers(ctx: &Ctx, arg: &str, page: &PageReq) -> Result<Page<User>> {
  relations(ctx, arg, FOLLOWERS, "followers", page).await
}

pub async fn following(ctx: &Ctx, arg: &str, page: &PageReq) -> Result<Page<User>> {
  relations(ctx, arg, FOLLOWING, "followings", page).await
}
