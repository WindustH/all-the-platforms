//! Managing the watch history and the watch-later list.

use atp_core::bulk;
use atp_core::{Action, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::Api;
use crate::paging::Pn;
use crate::parse;
use crate::refs::{self, Video};

const DELETE: &str = "https://api.bilibili.com/x/v2/history/delete";
const CLEAR: &str = "https://api.bilibili.com/x/v2/history/clear";
const PAUSE: &str = "https://api.bilibili.com/x/v2/history/shadow/set";
const SEARCH: &str = "https://api.bilibili.com/x/web-interface/history/search";
const LATER_ADD: &str = "https://api.bilibili.com/x/v2/history/toview/add";
const LATER_DEL: &str = "https://api.bilibili.com/x/v2/history/toview/v2/dels";
const LATER_CLEAR: &str = "https://api.bilibili.com/x/v2/history/toview/clear";

/// Videos given as posts, each through `op`, one call each.
async fn each_video(
  api: &Api,
  action: &str,
  posts: &[String],
  op: impl AsyncFn(&Video) -> Result<Action>,
) -> Result<Vec<Action>> {
  bulk::each(action, posts, async |post| {
    op(&refs::video(api, post).await?).await
  })
  .await
}

/// Remove videos from the history.
pub async fn delete(api: &Api, posts: &[String]) -> Result<Vec<Action>> {
  each_video(api, "history-delete", posts, async |v| {
    api
      .post(DELETE)
      .arg("kid", format!("archive_{}", v.aid))
      .arg("jsonp", "jsonp")
      .send()
      .await?;
    Ok(Action::done("history-delete", &v.bvid).with_url(v.url()))
  })
  .await
}

pub async fn clear(api: &Api) -> Result<Action> {
  api.post(CLEAR).arg("jsonp", "jsonp").send().await?;
  Ok(Action::done("history-clear", "history"))
}

/// Stop (or with `undo` resume) recording the watch history.
pub async fn pause(api: &Api, undo: bool) -> Result<Action> {
  api
    .post(PAUSE)
    .arg("switch", !undo)
    .arg("jsonp", "jsonp")
    .send()
    .await?;
  let action = if undo {
    "history-resume"
  } else {
    "history-pause"
  };
  Ok(Action::done(action, "history"))
}

/// History entries whose title matches `keyword`.
pub async fn search(api: &Api, keyword: &str, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let pn = Pn::of(page, 20);
  let data = api
    .get(SEARCH)
    .arg("pn", pn.pn)
    .arg("keyword", keyword)
    .arg("business", "all")
    .send()
    .await?;
  let posts: Vec<Post> = data
    .list("list")
    .iter()
    .map(|item| {
      let mut p = parse::video(item);
      if let Some(at) = item.i64("view_at") {
        p.extra.insert("viewed_at".into(), json!(at));
      }
      p
    })
    .filter(|p| !p.id.is_empty())
    .collect();
  let more = data.bool("has_more") == Some(true);
  Ok(pn.page(posts, more))
}

/// Put videos on the watch-later list.
pub async fn later_add(api: &Api, posts: &[String]) -> Result<Vec<Action>> {
  each_video(api, "watch-later", posts, async |v| {
    api.post(LATER_ADD).arg("aid", v.aid).send().await?;
    Ok(Action::done("watch-later", &v.bvid).with_url(v.url()))
  })
  .await
}

/// Take videos off the watch-later list.
pub async fn later_remove(api: &Api, posts: &[String]) -> Result<Vec<Action>> {
  each_video(api, "watch-later-remove", posts, async |v| {
    api.post(LATER_DEL).arg("resources", v.aid).send().await?;
    Ok(Action::done("watch-later-remove", &v.bvid).with_url(v.url()))
  })
  .await
}

/// Empty the list, or with `watched` only drop the videos already watched.
pub async fn later_clear(api: &Api, watched: bool) -> Result<Action> {
  let mut call = api.post(LATER_CLEAR);
  if watched {
    call = call.arg("clean_type", 2);
  }
  call.send().await?;
  let what = if watched {
    "watched videos"
  } else {
    "all videos"
  };
  Ok(Action::done("watch-later-clear", "watch later").with_message(what))
}
