//! Relations: whom a user follows and who follows them, as the profile's
//! relation panel lists them; your own following sorted, searched,
//! narrowed to mutual follows or to the accounts the panel's batch manager
//! calls invalid; and unfollowing several accounts at once.

use atp_core::{
  Action, Error, FollowQuery, Page, PageReq, Result, User, Value, ValueExt, bulk, json,
};

use crate::api::Api;
use crate::refs::{self, user_url};
use crate::{interactions, parse, users};

/// The message page's list of everyone you follow; its share dialog takes
/// the mutual follows from it too.
const CONTACTS: &str = "/api/im/web/users/following/all";
/// Your own following in the relation panel, in the order chosen there.
const OWN: &str = "/api/sns/web/v1/user/followings/self";
/// Your own following as the panel's batch manager (批量管理) splits it
/// into tabs.
const BATCH_LIST: &str = "/api/sns/web/v1/user/followings/batch_list";
const FOLLOWING: &str = "/api/sns/web/v1/user/followings";
const FOLLOWERS: &str = "/api/sns/web/v1/user/followers";
/// The relation panel's search box (搜索已关注的人).
const SEARCH: &str = "/api/sns/web/v1/search/user/follow";
const BATCH_UNFOLLOW: &str = "/api/sns/web/v1/followings/batch/unfollow";

/// `source` (and the batch unfollow's `scene`) of the relation panel.
const SOURCE: &str = "user_profile";
/// The batch manager's tab of the accounts it calls invalid.
const INVALID: &str = "invalid";
/// Users one batch unfollow takes (the batch manager's selection limit).
const BATCH: usize = 50;
/// Rows of a page of the message page's list, as it asks for them.
const CONTACT_PAGE: usize = 200;
/// Rows of a full page of the following search.
const SEARCH_PAGE: usize = 20;

/// The values of `following --sort`, the plain order first.
pub const SORTS: &[&str] = &["recent", "oldest", "smart"];

/// The relation panel's `order` for `--sort`: 0 latest follow first
/// (最新关注), 1 earliest (最早关注), 2 Xiaohongshu's own mix (综合排序).
fn order(sort: Option<&str>) -> Result<&'static str> {
  match sort {
    None | Some("recent") => Ok("0"),
    Some("oldest") => Ok("1"),
    Some("smart") => Ok("2"),
    Some(other) => Err(Error::input(format!(
      "unknown --sort {other} (recent, oldest or smart)"
    ))),
  }
}

/// `following`: your own from the message page's list (with `--sort`,
/// from the relation panel), someone else's from their relation panel;
/// `--search`, `--mutual` and the orders other than `recent` on your own
/// only.
pub async fn following(
  api: &Api,
  user: &str,
  query: &FollowQuery,
  req: &PageReq,
) -> Result<Page<User>> {
  api.require_login()?;
  let order = order(query.sort.as_deref())?;
  let narrowed = query.search.is_some() || query.mutual;
  if query.search.is_some() && query.mutual {
    return Err(Error::input("give --search or --mutual, not both"));
  }
  if narrowed && query.sort.is_some() {
    return Err(Error::input(
      "--search and --mutual have their own order; drop --sort",
    ));
  }
  let target = refs::user_ref(api, user).await?;
  let me = users::my_id(api).await?;
  if target != me {
    if narrowed || order != "0" {
      return Err(Error::input(
        "--sort oldest|smart, --search and --mutual work on your own following only",
      ));
    }
    return listed(api, FOLLOWING, &target, false, req).await;
  }
  match (&query.search, &query.sort) {
    (Some(words), _) => search(api, words, req).await,
    (None, _) if query.mutual => mutual(api, req).await,
    (None, Some(_)) => sorted(api, OWN, &me, order, "all", req).await,
    (None, None) => contacts(api, req).await,
  }
}

/// A user's followers, latest first, as their relation panel lists them.
pub async fn followers(api: &Api, user: &str, req: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let target = refs::user_ref(api, user).await?;
  let own = target == users::my_id(api).await?;
  listed(api, FOLLOWERS, &target, own, req).await
}

/// The accounts you follow that the batch manager files under its
/// `invalid` tab, latest follow first.
pub async fn invalid(api: &Api, req: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let me = users::my_id(api).await?;
  sorted(api, BATCH_LIST, &me, "0", INVALID, req).await
}

/// `follow` / `unfollow` of several users: following goes one by one,
/// unfollowing 50 per call, as the batch manager does.
pub async fn follow_users(api: &Api, users: &[String], undo: bool) -> Result<Vec<Action>> {
  if !undo {
    let follow = async |user: &str| interactions::follow(api, user, false).await;
    return bulk::each("follow", users, follow).await;
  }
  api.require_login()?;
  let mut ids = Vec::with_capacity(users.len());
  for user in users {
    ids.push(refs::user_ref(api, user).await?);
  }
  bulk::batches("unfollow", &ids, BATCH, String::clone, async |chunk| {
    api.write_pause().await;
    let body = json!({"source": SOURCE, "user_ids": chunk, "scene": SOURCE});
    api.post(BATCH_UNFOLLOW, &body).await?;
    let done = |id: &String| Action::done("unfollow", id).with_url(user_url(id));
    Ok(chunk.iter().map(done).collect())
  })
  .await
}

/// Everyone you follow from the message page's list, page by page.
async fn contacts(api: &Api, req: &PageReq) -> Result<Page<User>> {
  let page = req.number_or(1);
  let (rows, items) = contact_page(api, page, req.size_within(CONTACT_PAGE), false).await?;
  let next = (rows > 0).then(|| (page + 1).to_string());
  Ok(Page::new(items, next))
}

/// Your mutual follows: the message page's list read 200 at a time until
/// enough are found or a short page ends it, as its share dialog does.
async fn mutual(api: &Api, req: &PageReq) -> Result<Page<User>> {
  let mut page = req.number_or(1);
  let mut items = Vec::new();
  loop {
    let (rows, found) = contact_page(api, page, CONTACT_PAGE, true).await?;
    items.extend(found);
    page += 1;
    if rows < CONTACT_PAGE {
      return Ok(Page::last(items));
    }
    if items.len() >= req.size.max(1) {
      return Ok(Page::new(items, Some(page.to_string())));
    }
  }
}

/// One page of the message page's list: how many rows it held, and its
/// users (with `mutual`, the mutual follows among them, `status` 1).
async fn contact_page(
  api: &Api,
  page: u64,
  size: usize,
  mutual: bool,
) -> Result<(usize, Vec<User>)> {
  let (page, size) = (page.to_string(), size.to_string());
  let params = [("page", page.as_str()), ("size", size.as_str())];
  let data = api.get(CONTACTS, &params).await?;
  let rows = data.list("follow_user_d_t_o_list");
  let users = rows
    .iter()
    .filter(|row| !mutual || row.i64("status") == Some(1))
    .filter_map(parse::contact)
    .collect();
  Ok((rows.len(), users))
}

/// Your following as the relation panel lists it (`followings/self`, or
/// `batch_list` for its batch manager): one tab, in `order`.
async fn sorted(
  api: &Api,
  path: &str,
  me: &str,
  order: &str,
  tab: &str,
  req: &PageReq,
) -> Result<Page<User>> {
  let cursor = req.cursor.clone().unwrap_or_default();
  let params = [
    ("user_id", me),
    ("cursor", cursor.as_str()),
    ("order", order),
    ("tab_type", tab),
    ("source", SOURCE),
  ];
  let data = api.get(path, &params).await?;
  Ok(rows(&data, "user_list", true))
}

/// Whom a user follows or who follows them, latest first, as their
/// relation panel lists them; `own` for your own lists.
async fn listed(api: &Api, path: &str, user: &str, own: bool, req: &PageReq) -> Result<Page<User>> {
  let cursor = req.cursor.clone().unwrap_or_default();
  let params = [
    ("user_id", user),
    ("cursor", cursor.as_str()),
    ("order", "0"),
    ("source", SOURCE),
  ];
  let data = api.get(path, &params).await?;
  Ok(rows(&data, "users", own))
}

/// The accounts you follow whose name matches `words`, as the relation
/// panel's search box finds them, 20 a page.
async fn search(api: &Api, words: &str, req: &PageReq) -> Result<Page<User>> {
  let words = words.trim();
  if words.is_empty() {
    return Err(Error::input("--search needs words to look for"));
  }
  let page = req.number_or(1);
  let page_s = page.to_string();
  let params = [
    ("page", page_s.as_str()),
    ("source", "my_following"),
    ("keyword", words),
  ];
  let data = api.get(SEARCH, &params).await?;
  let rows = data.list("search_user_list");
  let items: Vec<User> = rows
    .iter()
    .filter_map(|row| parse::relation_user(row, true))
    .collect();
  let next = (rows.len() >= SEARCH_PAGE).then(|| (page + 1).to_string());
  Ok(Page::new(items, next))
}

/// A cursor-paged list of relation rows under `key`.
fn rows(data: &Value, key: &str, own: bool) -> Page<User> {
  let items = data
    .list(key)
    .iter()
    .filter_map(|row| parse::relation_user(row, own))
    .collect();
  let next = data
    .str("cursor")
    .filter(|_| data.bool("has_more") == Some(true));
  Page::new(items, next)
}
