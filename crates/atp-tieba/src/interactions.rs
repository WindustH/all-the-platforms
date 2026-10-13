//! Interactions as the logged-in account: agree (点赞), save (收藏), reply,
//! delete a reply, follow a user. Each pauses briefly first and sends the
//! account's `tbs`.

use atp_core::{Action, Reply, Result, ValueExt, json};

use crate::api::{Api, Params, WRITE, check_vcode};
use crate::refs::{floor_url, thread_url, user_url};
use crate::{refs, threads, uploads, users};

/// Agree (点赞) with a thread, or with one floor when the link names it.
pub async fn like(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let post = refs::post(arg)?;
  let tbs = api.tbs().await?;
  let thread = threads::context(api, &post.tid).await?;
  let (pid, obj_type) = match &post.pid {
    Some(p) => (p.clone(), "1"),
    None => (thread.first_pid.clone(), "3"),
  };
  api.write_pause().await;
  let params: Params = vec![
    ("thread_id", post.tid.clone()),
    ("post_id", pid.clone()),
    ("op_type", if undo { "1" } else { "0" }.into()),
    ("obj_type", obj_type.into()),
    ("agree_type", "2".into()),
    ("forum_id", thread.fid),
    ("tbs", tbs),
    ("stoken", api.stoken()),
  ];
  api.call("/c/c/agree/opAgree", params).await?;
  let url = match &post.pid {
    Some(p) => floor_url(&post.tid, p),
    None => thread_url(&post.tid),
  };
  Ok(Action::done(if undo { "unlike" } else { "like" }, &post.tid).with_url(url))
}

/// Save a thread (收藏), or remove it.
pub async fn favorite(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let post = refs::post(arg)?;
  let me = api.me().await?;
  api.write_pause().await;
  if undo {
    let params: Params = vec![
      ("tid", post.tid.clone()),
      ("fid", "null".into()),
      ("tbs", me.tbs),
      ("stoken", api.stoken()),
      ("user_id", me.uid),
    ];
    api.call("/c/c/post/rmstore", params).await?;
  } else {
    let pid = match post.pid.clone() {
      Some(p) => p,
      None => threads::context(api, &post.tid).await?.first_pid,
    };
    let data = json!([{ "tid": post.tid, "pid": pid, "status": 1 }]).to_string();
    let params: Params = vec![
      ("data", data),
      ("tbs", me.tbs),
      ("stoken", api.stoken()),
      ("user_id", me.uid),
    ];
    api.call("/c/c/post/addstore", params).await?;
  }
  let name = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(name, &post.tid).with_url(thread_url(&post.tid)))
}

/// Reply to a thread (a new floor), or inside a floor (楼中楼) with
/// `--reply-to FLOOR` or a link naming the floor.
pub async fn comment(api: &Api, arg: &str, reply: &Reply) -> Result<Action> {
  api.require_login()?;
  let post = refs::post(arg)?;
  let me = api.me().await?;
  let thread = threads::context(api, &post.tid).await?;
  let floor = match reply.reply_to.as_deref() {
    Some(to) => Some(
      refs::post(to)
        .ok()
        .and_then(|p| p.pid)
        .unwrap_or_else(|| to.trim().to_owned()),
    ),
    None => post.pid.clone(),
  };
  let content = uploads::with_images(api, &reply.text, &reply.images, &thread.forum).await?;
  api.write_pause().await;
  let mut params: Params = vec![
    ("content", content),
    ("fid", thread.fid.clone()),
    ("kw", thread.forum.clone()),
    ("tid", post.tid.clone()),
    ("tbs", me.tbs),
    ("stoken", api.stoken()),
    ("name_show", me.name),
    ("anonymous", "1".into()),
    ("authsid", "null".into()),
    ("can_no_forum", "0".into()),
    ("entrance_type", "0".into()),
    ("is_ad", "0".into()),
    ("is_barrage", "0".into()),
    ("is_feedback", "0".into()),
    ("new_vcode", "1".into()),
    ("post_from", "3".into()),
    ("reply_uid", "null".into()),
    ("takephoto_num", "0".into()),
    ("vcode_tag", "12".into()),
  ];
  if let Some(f) = &floor {
    params.push(("quote_id", f.clone()));
    params.push(("floor_num", "0".into()));
  } else {
    params.push(("barrage_time", "0".into()));
  }
  let v = api.call_as("/c/c/post/add", params, WRITE).await?;
  check_vcode(&v)?;
  let mut action = Action::done(if floor.is_some() { "reply" } else { "comment" }, &post.tid);
  match v.str("pid") {
    Some(pid) => action = action.with_url(floor_url(&post.tid, &pid)).with_id(pid),
    None => action = action.with_url(thread_url(&post.tid)),
  }
  Ok(action)
}

/// Delete one of your floors, or a reply inside a floor when POST names the
/// floor (`THREAD/FLOOR` or a `?pid=` link) and COMMENT is the reply.
pub async fn delete_comment(api: &Api, arg: &str, comment: &str) -> Result<Action> {
  api.require_login()?;
  let post = refs::post(arg)?;
  let pid = refs::post(comment)
    .ok()
    .and_then(|p| p.pid)
    .unwrap_or_else(|| comment.trim().to_owned());
  let inside = post.pid.as_deref().is_some_and(|f| f != pid);
  let tbs = api.tbs().await?;
  let thread = threads::context(api, &post.tid).await?;
  api.write_pause().await;
  let params: Params = vec![
    ("fid", thread.fid),
    ("word", thread.forum),
    ("z", post.tid.clone()),
    ("pid", pid.clone()),
    ("isfloor", if inside { "1" } else { "0" }.into()),
    ("src", if inside { "3" } else { "1" }.into()),
    ("is_vipdel", "0".into()),
    ("delete_my_post", "1".into()),
    ("tbs", tbs),
  ];
  api.call("/c/c/bawu/delpost", params).await?;
  Ok(Action::done("delete-comment", &pid).with_url(thread_url(&post.tid)))
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let portrait = users::portrait(api, arg).await?;
  let tbs = api.tbs().await?;
  api.write_pause().await;
  let params: Params = vec![
    ("portrait", portrait.clone()),
    ("tbs", tbs),
    ("stoken", api.stoken()),
    ("authsid", "null".into()),
    ("from_type", "2".into()),
    ("in_live", "0".into()),
  ];
  let path = if undo {
    "/c/c/user/unfollow"
  } else {
    "/c/c/user/follow"
  };
  api.call_as(path, params, WRITE).await?;
  Ok(
    Action::done(if undo { "unfollow" } else { "follow" }, &portrait).with_url(user_url(&portrait)),
  )
}
