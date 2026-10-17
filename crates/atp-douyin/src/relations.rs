//! Your relations: your following sorted, searched or narrowed to mutual
//! follows, removing followers, blocking, and approving follow requests.

use atp_core::bulk;
use atp_core::http::Method;
use atp_core::{Action, Error, FollowQuery, Page, PageReq, Result, User, Value, ValueExt, paging};

use crate::api::Api;
use crate::parse;
use crate::refs::user_url;
use crate::users::{self, Order};

const SUGGEST: &str = "/aweme/v1/web/api/suggest_words/";
const SPOTLIGHT: &str = "/aweme/v1/web/im/spotlight/relation/";
const REMOVE_FOLLOWER: &str = "/aweme/v1/web/remove/follower/";
const BLOCK: &str = "/aweme/v1/web/user/block/";
const APPROVE: &str = "/aweme/v1/web/commit/follow/request/approve/";

/// The suggestion service of the following page's search box.
const FOLLOWING_SEARCH: &str = "90062";

/// `following` with `--sort`, `--search` or `--mutual`: the last two and
/// the orders other than `recent` only for your own list.
pub async fn following(
  api: &Api,
  user: &str,
  query: &FollowQuery,
  page: &PageReq,
) -> Result<Page<User>> {
  let order = Order::parse(query.sort.as_deref())?;
  let narrowed = query.search.is_some() || query.mutual;
  if query.search.is_some() && query.mutual {
    return Err(Error::input("give --search or --mutual, not both"));
  }
  if narrowed && query.sort.is_some() {
    return Err(Error::input(
      "--search and --mutual have their own order; drop --sort",
    ));
  }
  if (narrowed || order != Order::Recent) && !users::is_me(api, user).await? {
    return Err(Error::input(
      "--sort oldest|smart, --search and --mutual work on your own following only",
    ));
  }
  match (&query.search, query.mutual) {
    (Some(words), _) => search(api, words, page).await,
    (None, true) => mutual(api, page).await,
    (None, false) => users::following(api, user, order, page).await,
  }
}

/// The accounts you follow whose name or Douyin id matches, as the
/// following page's search box finds them (at most 100).
async fn search(api: &Api, words: &str, page: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let v = api
    .get(SUGGEST)
    .arg("count", 100)
    .arg("query", words)
    .arg("business_id", FOLLOWING_SEARCH)
    .arg("category_name", "aweme_at_user")
    .arg("words_source", "aweme_at_user")
    .arg("pd", "aweme_at_user")
    .send()
    .await?;
  if let Some(code) = v.i64("errno").filter(|c| *c != 0) {
    return Err(Error::upstream(format!(
      "Douyin did not search your following (errno {code})"
    )));
  }
  let users: Vec<User> = v
    .list("data.0.words")
    .iter()
    .filter_map(parse::suggested_user)
    .filter(|(_, status)| status == "follow" || status == "friends")
    .map(|(user, _)| user)
    .collect();
  Ok(paging::slice(users, page))
}

/// Your mutual follows, from the relations the share panel lists
/// (`follow_status` 2), newest first.
async fn mutual(api: &Api, page: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let now = jiff::Timestamp::now().as_millisecond().to_string();
  let max_time = page.cursor.clone().unwrap_or(now);
  let v = api
    .get(SPOTLIGHT)
    .arg("count", 50)
    .arg("source", "coldup")
    .arg("max_time", &max_time)
    .arg("min_time", 0)
    .arg("need_remove_share_panel", "true")
    .arg("need_sorted_info", "true")
    .arg("with_fstatus", 1)
    .send()
    .await?;
  let rows = v.list("followings");
  let users: Vec<User> = rows
    .iter()
    .filter(|row| row.i64("follow_status") == Some(2))
    .filter_map(parse::relation)
    .collect();
  let next = (has_more(&v) && !rows.is_empty())
    .then(|| v.str("min_time"))
    .flatten()
    .filter(|t| *t != max_time && t != "0");
  Ok(Page::new(users, next))
}

fn has_more(v: &Value) -> bool {
  v.bool("has_more") == Some(true) || v.i64("has_more") == Some(1)
}

/// Make users stop following you, one by one.
pub async fn remove_followers(api: &Api, users: &[String]) -> Result<Vec<Action>> {
  bulk::each("remove-follower", users, async |user| {
    let sec = users::sec_uid(api, user).await?;
    let me = users::uid_hash(api).await?;
    api
      .write_as(Method::POST, REMOVE_FOLLOWER, &me)
      .await?
      .header("referer", user_url(&sec))
      .arg("sec_user_id", &sec)
      .send()
      .await?;
    Ok(Action::done("remove-follower", &sec).with_url(user_url(&sec)))
  })
  .await
}

/// Block (or with `undo` unblock) users, one by one.
pub async fn block(api: &Api, users: &[String], undo: bool) -> Result<Vec<Action>> {
  let name = if undo { "unblock" } else { "block" };
  bulk::each(name, users, async |user| {
    let (sec, uid) = users::uid_of(api, user).await?;
    let me = users::uid_hash(api).await?;
    api
      .write_as(Method::POST, BLOCK, &me)
      .await?
      .header("referer", user_url(&sec))
      .field("user_id", &uid)
      .field("sec_user_id", &sec)
      .field("block_type", if undo { 0 } else { 1 })
      .field("source", 0)
      .send()
      .await?;
    Ok(Action::done(name, &sec).with_url(user_url(&sec)))
  })
  .await
}

/// Let users who asked to follow your private account follow it.
pub async fn approve(api: &Api, users: &[String]) -> Result<Vec<Action>> {
  bulk::each("approve-follower", users, async |user| {
    let (sec, uid) = users::uid_of(api, user).await?;
    let me = users::uid_hash(api).await?;
    api
      .write_as(Method::POST, APPROVE, &me)
      .await?
      .header("referer", user_url(&sec))
      .field("from_user_id", &uid)
      .send()
      .await?;
    Ok(Action::done("approve-follower", &sec).with_url(user_url(&sec)))
  })
  .await
}
