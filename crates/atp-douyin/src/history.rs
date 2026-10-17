//! What you watched and what you keep for later: the viewing history and
//! clearing it, and the watch-later list (稍后再看) with its adding and
//! removing.

use atp_core::bulk;
use atp_core::http::Method;
use atp_core::{Action, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::{Api, WWW};
use crate::{parse, refs, users};

const HISTORY: &str = "/aweme/v1/web/history/read/";
const CLEAR: &str = "/aweme/v1/web/history/clear/";
const LATER: &str = "/aweme/v1/web/watchlater/list/";
const LATER_ADD: &str = "/aweme/v1/web/watchlater/upsert/";
const LATER_REMOVE: &str = "/aweme/v1/web/watchlater/remove/";

/// Posts one removal from watch later takes.
const BATCH: usize = 50;

/// What `watchlater/remove` takes out: the posts named, the ones watched,
/// or everything.
#[derive(Debug, Clone, Copy)]
enum Removal {
  Selected = 0,
  Watched = 1,
  All = 2,
}

pub async fn list(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let v = api
    .get(HISTORY)
    .arg("max_cursor", page.number_or(0))
    .arg("count", page.size_within(20))
    .send()
    .await?;
  Ok(parse::awemes(&v, "aweme_list", "max_cursor"))
}

/// Erase the whole viewing history (the page offers nothing finer).
pub async fn clear(api: &Api) -> Result<Action> {
  let me = users::uid_hash(api).await?;
  api
    .write_as(Method::POST, CLEAR, &me)
    .await?
    .header("referer", format!("{WWW}/user/self"))
    .send()
    .await?;
  Ok(Action::done("history-clear", "history"))
}

/// Your watch-later list, latest added first.
pub async fn later(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let v = api
    .get(LATER)
    .arg("offset", page.number_or(0))
    .arg("list_type", 0)
    .arg("operate_type", 0)
    .send()
    .await?;
  Ok(parse::awemes(&v, "items", "offset"))
}

/// Put posts on the watch-later list, one call each as the player does.
pub async fn later_add(api: &Api, posts: &[String]) -> Result<Vec<Action>> {
  bulk::each("later-add", posts, async |post| {
    let id = refs::aweme(api, post).await?;
    let me = users::uid_hash(api).await?;
    let body = json!({
      "item_to_playinfo": { id.as_str(): { "play_duration": 0, "loop_status": 0 } },
      "action_type": 1,
    });
    api
      .write_as(Method::POST, LATER_ADD, &me)
      .await?
      .header("referer", refs::video_url(&id))
      .json(&body)
      .send()
      .await?;
    Ok(Action::done("later-add", &id).with_url(refs::video_url(&id)))
  })
  .await
}

/// Take posts off the watch-later list, 50 per call.
pub async fn later_remove(api: &Api, posts: &[String]) -> Result<Vec<Action>> {
  let ids = refs::awemes(api, posts).await?;
  let me = users::uid_hash(api).await?;
  bulk::batches("later-remove", &ids, BATCH, String::clone, async |chunk| {
    remove(api, &me, chunk, Removal::Selected).await?;
    let done = |id: &String| Action::done("later-remove", id).with_url(refs::video_url(id));
    Ok(chunk.iter().map(done).collect())
  })
  .await
}

/// Empty the watch-later list, or with `watched` drop only what you
/// watched; the message says how many went.
pub async fn later_clear(api: &Api, watched: bool) -> Result<Action> {
  let me = users::uid_hash(api).await?;
  let how = if watched {
    Removal::Watched
  } else {
    Removal::All
  };
  let removed = remove(api, &me, &[], how).await?;
  let target = if watched { "watched" } else { "all" };
  Ok(Action::done("later-clear", target).with_message(format!("{removed} removed")))
}

/// One `watchlater/remove` call; how many it removed.
async fn remove(api: &Api, me: &str, ids: &[String], how: Removal) -> Result<u64> {
  let body = json!({ "remove_item_ids": ids, "remove_type": how as u8 });
  let v = api
    .write_as(Method::POST, LATER_REMOVE, me)
    .await?
    .header("referer", format!("{WWW}/user/self"))
    .json(&body)
    .send()
    .await?;
  Ok(v.u64("removed_num").unwrap_or_default())
}
