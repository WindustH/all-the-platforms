//! Users: profiles, the logged-in account, their posts and liked posts,
//! followers and following (your own in three orders); following and
//! unfollowing.

use atp_core::{Action, Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt};
use md5::{Digest, Md5};

use crate::api::Api;
use crate::refs::{self, UserArg};
use crate::{discover, parse};

const PROFILE: &str = "/aweme/v1/web/user/profile/other/";
const SELF: &str = "/aweme/v1/web/user/profile/self/";
const POSTS: &str = "/aweme/v1/web/aweme/post/";
const LIKES: &str = "/aweme/v1/web/aweme/favorite/";
const FOLLOWERS: &str = "/aweme/v1/web/user/follower/list/";
const FOLLOWING: &str = "/aweme/v1/web/user/following/list/";
const FOLLOW: &str = "/aweme/v1/web/commit/follow/user/";

/// The `sec_uid` of a user reference; `@douyin_id` is looked up by search.
pub async fn sec_uid(api: &Api, arg: &str) -> Result<String> {
  match refs::user(api, arg).await? {
    UserArg::SecUid(s) => Ok(s),
    UserArg::Handle(handle) => {
      let q = Query {
        keyword: handle.clone(),
        ..Query::default()
      };
      let found = discover::search_users(
        api,
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

async fn profile_raw(api: &Api, sec: &str) -> Result<Value> {
  let v = api
    .get(PROFILE)
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

pub async fn profile(api: &Api, arg: &str) -> Result<User> {
  let sec = sec_uid(api, arg).await?;
  let v = profile_raw(api, &sec).await?;
  parse::user(&v).ok_or_else(|| Error::upstream("Douyin returned a user without a sec_uid"))
}

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let v = api
    .get(SELF)
    .arg("publish_video_strategy_type", "2")
    .arg("source", "channel_pc_web")
    .arg("personal_center_strategy", "1")
    .send()
    .await?;
  parse::user(v.at("user")).ok_or_else(|| {
    Error::auth("Douyin did not return the logged-in account")
      .with_hint("run `atp douyin login` (QR code) or `atp douyin login --browser`")
  })
}

/// A `max_cursor` listing of awemes (posts, likes).
async fn awemes(api: &Api, path: &str, sec: &str, page: &PageReq, own: bool) -> Result<Page<Post>> {
  let cursor = page.number_or(0);
  let mut call = api
    .get(path)
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

pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let sec = sec_uid(api, arg).await?;
  awemes(api, POSTS, &sec, page, false).await
}

/// Posts a user liked (public only when the user allows it).
pub async fn likes(api: &Api, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let sec = match user {
    Some(u) => sec_uid(api, u).await?,
    None => whoami(api).await?.id,
  };
  awemes(api, LIKES, &sec, page, user.is_none()).await
}

/// The orders of a following list (`source_type`), `following --sort`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
  /// Latest follow first, paged by `max_time` (1).
  Recent,
  /// Earliest follow first, paged by `min_time` (3).
  Oldest,
  /// Douyin's own mix (综合排序), paged by `offset` (4).
  Smart,
}

/// The values of `following --sort`, the plain order first.
pub const SORTS: &[&str] = &["recent", "oldest", "smart"];

impl Order {
  pub fn parse(sort: Option<&str>) -> Result<Self> {
    match sort {
      None | Some("recent") => Ok(Order::Recent),
      Some("oldest") => Ok(Order::Oldest),
      Some("smart") => Ok(Order::Smart),
      Some(other) => Err(Error::input(format!(
        "unknown --sort {other} (recent, oldest or smart)"
      ))),
    }
  }

  fn source_type(self) -> &'static str {
    match self {
      Order::Recent => "1",
      Order::Oldest => "3",
      Order::Smart => "4",
    }
  }

  /// `max_time`, `min_time` and `offset` of the page at `cursor`. The
  /// first page asks with the current time as `max_time` (`0` returned an
  /// empty list).
  fn window(self, cursor: Option<&str>) -> [String; 3] {
    let now = jiff::Timestamp::now().as_second().to_string();
    let at = |first: &str| cursor.unwrap_or(first).to_owned();
    match self {
      Order::Recent => [at(&now), "0".into(), "0".into()],
      Order::Oldest => [now, at("0"), "0".into()],
      Order::Smart => [now, "0".into(), at("0")],
    }
  }

  /// The answer's field the next page starts from.
  fn next_key(self) -> &'static str {
    match self {
      Order::Recent => "min_time",
      Order::Oldest => "max_time",
      Order::Smart => "offset",
    }
  }
}

/// Followers or followings in `order`, each row with what it tells about
/// the relation to you (`parse::relation`).
async fn relations(
  api: &Api,
  arg: &str,
  path: &str,
  key: &str,
  order: Order,
  page: &PageReq,
) -> Result<Page<User>> {
  let (sec, uid) = uid_of(api, arg).await?;
  let [max_time, min_time, offset] = order.window(page.cursor.as_deref());
  let mut call = api
    .get(path)
    .arg("user_id", &uid)
    .arg("sec_user_id", &sec)
    .arg("offset", offset)
    .arg("min_time", min_time)
    .arg("max_time", max_time)
    .arg("count", page.size_within(20))
    .arg("source_type", order.source_type())
    .arg("gps_access", "0")
    .arg("address_book_access", "0");
  if path == FOLLOWING {
    call = call.arg("is_top", "1");
  }
  let v = call.send().await?;
  let items: Vec<User> = v.list(key).iter().filter_map(parse::relation).collect();
  let next = (v.i64("has_more") == Some(1) && !items.is_empty())
    .then(|| v.str(order.next_key()))
    .flatten()
    .filter(|c| Some(c.as_str()) != page.cursor.as_deref() && c != "0");
  Ok(Page::new(items, next))
}

pub async fn followers(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  relations(api, arg, FOLLOWERS, "followers", Order::Recent, page).await
}

/// Whom a user follows; orders other than the latest first only for your
/// own list, as the profile page offers them.
pub async fn following(api: &Api, arg: &str, order: Order, page: &PageReq) -> Result<Page<User>> {
  relations(api, arg, FOLLOWING, "followings", order, page).await
}

/// Whether a user reference is the logged-in account.
pub async fn is_me(api: &Api, arg: &str) -> Result<bool> {
  let sec = sec_uid(api, arg).await?;
  Ok(whoami(api).await?.id == sec)
}

/// The `sec_uid` and numeric uid of a user reference, from the profile.
pub async fn uid_of(api: &Api, arg: &str) -> Result<(String, String)> {
  let sec = sec_uid(api, arg).await?;
  let uid = profile_raw(api, &sec)
    .await?
    .str("uid")
    .ok_or_else(|| Error::upstream("profile without uid"))?;
  Ok((sec, uid))
}

/// The logged-in account's uid, looked up once per command.
pub async fn uid(api: &Api) -> Result<String> {
  if let Some(uid) = api.me.get() {
    return Ok(uid.clone());
  }
  let me = whoami(api).await?;
  let uid = me
    .extra
    .get("uid")
    .and_then(Value::as_str)
    .map(str::to_owned)
    .ok_or_else(|| Error::upstream("Douyin returned the account without its uid"))?;
  Ok(api.me.get_or_init(|| uid).clone())
}

/// The MD5 of the logged-in account's uid, which the web client appends
/// to its writes.
pub async fn uid_hash(api: &Api) -> Result<String> {
  let uid = uid(api).await?;
  Ok(
    Md5::digest(uid.as_bytes())
      .iter()
      .map(|b| format!("{b:02x}"))
      .collect(),
  )
}

/// Follow a user, or with `undo` unfollow.
pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let (sec, uid) = uid_of(api, arg).await?;
  let me = uid_hash(api).await?;
  api
    .write(FOLLOW, false, &me)
    .await?
    .header("referer", refs::user_url(&sec))
    .field("type", if undo { "0" } else { "1" })
    .field("user_id", &uid)
    .send()
    .await?;
  let name = if undo { "unfollow" } else { "follow" };
  Ok(Action::done(name, &sec).with_url(refs::user_url(&sec)))
}
