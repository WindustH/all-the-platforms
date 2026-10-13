//! Managing the watch history and the watch-later list.

use social_core::{Action, Ctx, Page, PageReq, Post, Result, ValueExt, json};

use crate::page::Pn;
use crate::refs::Video;
use crate::{api, parse};

const DELETE: &str = "https://api.bilibili.com/x/v2/history/delete";
const CLEAR: &str = "https://api.bilibili.com/x/v2/history/clear";
const PAUSE: &str = "https://api.bilibili.com/x/v2/history/shadow/set";
const SEARCH: &str = "https://api.bilibili.com/x/web-interface/history/search";
const LATER_ADD: &str = "https://api.bilibili.com/x/v2/history/toview/add";
const LATER_DEL: &str = "https://api.bilibili.com/x/v2/history/toview/v2/dels";
const LATER_CLEAR: &str = "https://api.bilibili.com/x/v2/history/toview/clear";

/// Remove one video from the history.
pub async fn delete(ctx: &Ctx, v: &Video) -> Result<Action> {
  api::post(ctx, DELETE)
    .arg("kid", format!("archive_{}", v.aid))
    .arg("jsonp", "jsonp")
    .send()
    .await?;
  Ok(Action::done("history-delete", &v.bvid).with_url(v.url()))
}

pub async fn clear(ctx: &Ctx) -> Result<Action> {
  api::post(ctx, CLEAR).arg("jsonp", "jsonp").send().await?;
  Ok(Action::done("history-clear", "history"))
}

/// Stop (or with `undo` resume) recording the watch history.
pub async fn pause(ctx: &Ctx, undo: bool) -> Result<Action> {
  api::post(ctx, PAUSE)
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
pub async fn search(ctx: &Ctx, keyword: &str, page: &PageReq) -> Result<Page<Post>> {
  ctx.require_login(api::LOGIN_COOKIES)?;
  let pn = Pn::of(page, 20);
  let data = api::get(ctx, SEARCH)
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

pub async fn later_add(ctx: &Ctx, v: &Video) -> Result<Action> {
  api::post(ctx, LATER_ADD).arg("aid", v.aid).send().await?;
  Ok(Action::done("watch-later", &v.bvid).with_url(v.url()))
}

pub async fn later_remove(ctx: &Ctx, v: &Video) -> Result<Action> {
  api::post(ctx, LATER_DEL)
    .arg("resources", v.aid)
    .send()
    .await?;
  Ok(Action::done("watch-later-remove", &v.bvid).with_url(v.url()))
}

/// Empty the list, or with `watched` only drop the videos already watched.
pub async fn later_clear(ctx: &Ctx, watched: bool) -> Result<Action> {
  let mut call = api::post(ctx, LATER_CLEAR);
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
