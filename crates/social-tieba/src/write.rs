//! Writes as the logged-in account: agree (点赞), save (收藏), reply, post a
//! thread, delete, follow users and forums, sign in (签到), and the picture
//! uploads replies and threads carry. Each pauses briefly first, and every
//! write sends the account's `tbs`.

use std::path::Path;
use std::time::Duration;

use md5::{Digest, Md5};
use social_core::file::Image;
use social_core::http::Part;
use social_core::{Action, Draft, Error, ErrorCode, Reply, Result, Value, ValueExt, json};

use crate::api::{Client, Params, WRITE};
use crate::refs::{self, floor_url, forum_url, thread_url, user_url};
use crate::{forum, people, thread};

const CHUNK: usize = 512_000;

async fn pause(c: &Client) {
  c.ctx
    .http
    .pause(Duration::from_millis(800), Duration::from_millis(2000))
    .await;
}

/// A captcha the app would show means stop and use the app or browser.
fn vcode(v: &Value) -> Result<()> {
  if v.str("info.need_vcode").as_deref() == Some("1") {
    return Err(
      Error::new(
        ErrorCode::VerificationRequired,
        "Tieba asks for a captcha for this post",
      )
      .with_hint("post it once in the app or browser, wait a while, then retry"),
    );
  }
  Ok(())
}

/// Agree (点赞) with a thread, or with one floor when the link names it.
pub async fn like(c: &Client, arg: &str, undo: bool) -> Result<Action> {
  c.require_login()?;
  let r = refs::post(arg)?;
  let tbs = c.tbs().await?;
  let t = thread::context(c, &r.tid).await?;
  let (pid, obj_type) = match &r.pid {
    Some(p) => (p.clone(), "1"),
    None => (t.first_pid.clone(), "3"),
  };
  pause(c).await;
  let params: Params = vec![
    ("thread_id", r.tid.clone()),
    ("post_id", pid.clone()),
    ("op_type", if undo { "1" } else { "0" }.into()),
    ("obj_type", obj_type.into()),
    ("agree_type", "2".into()),
    ("forum_id", t.fid),
    ("tbs", tbs),
    ("stoken", c.stoken()),
  ];
  c.call("/c/c/agree/opAgree", params).await?;
  let url = match &r.pid {
    Some(p) => floor_url(&r.tid, p),
    None => thread_url(&r.tid),
  };
  Ok(Action::done(if undo { "unlike" } else { "like" }, &r.tid).with_url(url))
}

/// Save a thread (收藏), or remove it.
pub async fn favorite(c: &Client, arg: &str, undo: bool) -> Result<Action> {
  c.require_login()?;
  let r = refs::post(arg)?;
  let me = c.me().await?;
  pause(c).await;
  if undo {
    let params: Params = vec![
      ("tid", r.tid.clone()),
      ("fid", "null".into()),
      ("tbs", me.tbs),
      ("stoken", c.stoken()),
      ("user_id", me.uid),
    ];
    c.call("/c/c/post/rmstore", params).await?;
  } else {
    let pid = match r.pid.clone() {
      Some(p) => p,
      None => thread::context(c, &r.tid).await?.first_pid,
    };
    let data = json!([{ "tid": r.tid, "pid": pid, "status": 1 }]).to_string();
    let params: Params = vec![
      ("data", data),
      ("tbs", me.tbs),
      ("stoken", c.stoken()),
      ("user_id", me.uid),
    ];
    c.call("/c/c/post/addstore", params).await?;
  }
  let name = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(name, &r.tid).with_url(thread_url(&r.tid)))
}

/// Reply to a thread (a new floor), or inside a floor (楼中楼) with
/// `--reply-to FLOOR` or a link naming the floor.
pub async fn comment(c: &Client, arg: &str, reply: &Reply) -> Result<Action> {
  c.require_login()?;
  let r = refs::post(arg)?;
  let me = c.me().await?;
  let t = thread::context(c, &r.tid).await?;
  let floor = match reply.reply_to.as_deref() {
    Some(to) => Some(
      refs::post(to)
        .ok()
        .and_then(|p| p.pid)
        .unwrap_or_else(|| to.trim().to_owned()),
    ),
    None => r.pid.clone(),
  };
  let content = with_images(c, &reply.text, &reply.images, &t.forum).await?;
  pause(c).await;
  let mut params: Params = vec![
    ("content", content),
    ("fid", t.fid.clone()),
    ("kw", t.forum.clone()),
    ("tid", r.tid.clone()),
    ("tbs", me.tbs),
    ("stoken", c.stoken()),
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
  let v = c.call_as("/c/c/post/add", params, WRITE).await?;
  vcode(&v)?;
  let mut action = Action::done(if floor.is_some() { "reply" } else { "comment" }, &r.tid);
  match v.str("pid") {
    Some(pid) => action = action.with_url(floor_url(&r.tid, &pid)).with_id(pid),
    None => action = action.with_url(thread_url(&r.tid)),
  }
  Ok(action)
}

/// Delete one of your floors, or a reply inside a floor when POST names the
/// floor (`THREAD/FLOOR` or a `?pid=` link) and COMMENT is the reply.
pub async fn delete_comment(c: &Client, arg: &str, comment: &str) -> Result<Action> {
  c.require_login()?;
  let r = refs::post(arg)?;
  let pid = refs::post(comment)
    .ok()
    .and_then(|p| p.pid)
    .unwrap_or_else(|| comment.trim().to_owned());
  let inside = r.pid.as_deref().is_some_and(|f| f != pid);
  let tbs = c.tbs().await?;
  let t = thread::context(c, &r.tid).await?;
  pause(c).await;
  let params: Params = vec![
    ("fid", t.fid),
    ("word", t.forum),
    ("z", r.tid.clone()),
    ("pid", pid.clone()),
    ("isfloor", if inside { "1" } else { "0" }.into()),
    ("src", if inside { "3" } else { "1" }.into()),
    ("is_vipdel", "0".into()),
    ("delete_my_post", "1".into()),
    ("tbs", tbs),
  ];
  c.call("/c/c/bawu/delpost", params).await?;
  Ok(Action::done("delete-comment", &pid).with_url(thread_url(&r.tid)))
}

/// Post a thread to the forum named by `--topic`.
pub async fn publish(c: &Client, draft: &Draft) -> Result<Action> {
  c.require_login()?;
  if draft.reply_to.is_some() || draft.quote.is_some() {
    return Err(
      Error::unsupported("post --reply-to / --quote")
        .with_hint("reply with `social tieba comment THREAD TEXT`"),
    );
  }
  let target = draft
    .topics
    .first()
    .ok_or_else(|| Error::input("a Tieba thread needs its forum: `--topic FORUM`"))?;
  let (fid, kw) = forum::fid(c, target).await?;
  let me = c.me().await?;
  let content = with_images(c, &draft.text, &draft.images, &kw).await?;
  let title = draft.title.clone().unwrap_or_default();
  pause(c).await;
  let params: Params = vec![
    ("content", content),
    ("kw", kw.clone()),
    ("fid", fid),
    ("title", title.clone()),
    ("is_ntitle", if title.is_empty() { "1" } else { "0" }.into()),
    ("is_hide", "0".into()),
    ("is_feedback", "0".into()),
    ("reply_uid", "null".into()),
    ("takephoto_num", "0".into()),
    ("entrance_type", "1".into()),
    ("vcode_tag", "12".into()),
    ("new_vcode", "1".into()),
    ("anonymous", "1".into()),
    ("call_from", "2".into()),
    ("can_no_forum", "0".into()),
    ("name_show", me.name),
    ("tbs", me.tbs),
    ("stoken", c.stoken()),
  ];
  let v = c.call_as("/c/c/thread/add", params, WRITE).await?;
  vcode(&v)?;
  let mut action = Action::done("publish", if title.is_empty() { &kw } else { &title });
  if let Some(tid) = v.str("tid") {
    action = action.with_url(thread_url(&tid)).with_id(tid);
  }
  Ok(action)
}

/// Delete one of your threads.
pub async fn delete(c: &Client, arg: &str) -> Result<Action> {
  c.require_login()?;
  let r = refs::post(arg)?;
  let tbs = c.tbs().await?;
  let t = thread::context(c, &r.tid).await?;
  pause(c).await;
  let params: Params = vec![
    ("fid", t.fid),
    ("word", t.forum),
    ("z", r.tid.clone()),
    ("tbs", tbs),
    ("src", "1".into()),
    ("is_vipdel", "0".into()),
    ("delete_my_thread", "1".into()),
    ("is_frs_mask", "0".into()),
  ];
  c.call("/c/c/bawu/delthread", params).await?;
  Ok(Action::done("delete", &r.tid))
}

pub async fn follow(c: &Client, arg: &str, undo: bool) -> Result<Action> {
  c.require_login()?;
  let portrait = people::portrait(c, arg).await?;
  let tbs = c.tbs().await?;
  pause(c).await;
  let params: Params = vec![
    ("portrait", portrait.clone()),
    ("tbs", tbs),
    ("stoken", c.stoken()),
    ("authsid", "null".into()),
    ("from_type", "2".into()),
    ("in_live", "0".into()),
  ];
  let path = if undo {
    "/c/c/user/unfollow"
  } else {
    "/c/c/user/follow"
  };
  c.call_as(path, params, WRITE).await?;
  Ok(
    Action::done(if undo { "unfollow" } else { "follow" }, &portrait).with_url(user_url(&portrait)),
  )
}

/// Follow (关注) a forum, or leave it.
pub async fn follow_forum(c: &Client, arg: &str, undo: bool) -> Result<Action> {
  c.require_login()?;
  let (fid, kw) = forum::fid(c, arg).await?;
  let tbs = c.tbs().await?;
  pause(c).await;
  let params: Params = vec![("fid", fid), ("kw", kw.clone()), ("tbs", tbs)];
  let path = if undo {
    "/c/c/forum/unlike"
  } else {
    "/c/c/forum/like"
  };
  c.call(path, params).await?;
  let name = if undo {
    "unfollow-forum"
  } else {
    "follow-forum"
  };
  Ok(Action::done(name, &kw).with_url(forum_url(&kw)))
}

/// Sign in (签到) to one forum: the result row of `sign`.
pub async fn sign(c: &Client, arg: &str) -> Result<Value> {
  c.require_login()?;
  let (fid, kw) = forum::fid(c, arg).await?;
  let tbs = c.tbs().await?;
  pause(c).await;
  let params: Params = vec![("kw", kw.clone()), ("fid", fid), ("tbs", tbs)];
  let v = c.call_raw("/c/c/forum/sign", params, WRITE).await?;
  let code = v.str("error_code").unwrap_or_else(|| "0".into());
  let msg = v
    .first_str(&["error_msg", "error.usermsg"])
    .unwrap_or_default();
  // 160002: already signed today.
  let ok = code == "0" || code == "160002";
  if !ok && code != "340006" {
    // Hard failures (session, captcha, rate limit) stop a batch.
    let e = c.error(&code, &msg);
    if matches!(
      e.code,
      ErrorCode::NotAuthenticated | ErrorCode::VerificationRequired | ErrorCode::RateLimited
    ) {
      return Err(e);
    }
  }
  let info = v.at("user_info");
  Ok(json!({
    "forum": kw,
    "ok": ok,
    "already": code == "160002",
    "message": (!msg.is_empty()).then_some(msg),
    "bonus": info.u64("sign_bonus_point"),
    "days": info.u64("cont_sign_num"),
    "total_days": info.u64("total_sign_num"),
    "rank": info.u64("user_sign_rank"),
  }))
}

/// Sign in to every followed forum, one after the other.
pub async fn sign_all(c: &Client) -> Result<Vec<Value>> {
  let all = social_core::paging::collect(usize::MAX, None, async |r| {
    people::forums(c, None, &r).await
  })
  .await?;
  let mut rows = Vec::new();
  for f in all.items {
    rows.push(sign(c, &f.name).await?);
  }
  Ok(rows)
}

/// The text with uploaded pictures appended as `#(pic,id,w,h)` markers.
async fn with_images(
  c: &Client,
  text: &str,
  images: &[impl AsRef<Path>],
  forum: &str,
) -> Result<String> {
  let mut out = text.to_owned();
  for path in images {
    let marker = upload(c, path.as_ref(), forum).await?;
    if !out.is_empty() {
      out.push('\n');
    }
    out += &marker;
  }
  if out.trim().is_empty() {
    return Err(Error::input("nothing to send: give a text or --image"));
  }
  Ok(out)
}

/// Upload a picture in 512 KB chunks, as the app does.
async fn upload(c: &Client, path: &Path, forum: &str) -> Result<String> {
  let image = Image::read(path).await?;
  let (width, height) = image.size();
  let size = image.data.len();
  let md5: String = Md5::digest(&image.data)
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect();
  let chunks: Vec<&[u8]> = image.data.chunks(CHUNK).collect();
  let mut last = Value::Null;
  for (i, chunk) in chunks.iter().enumerate() {
    let finish = i + 1 == chunks.len();
    let params: Params = vec![
      ("alt", "json".into()),
      ("chunkNo", (i + 1).to_string()),
      ("forum_name", forum.to_owned()),
      ("groupId", "1".into()),
      ("height", height.to_string()),
      ("isFinish", if finish { "1" } else { "0" }.into()),
      ("is_bjh", "0".into()),
      ("pic_water_type", "0".into()),
      ("resourceId", format!("{md5}{CHUNK}")),
      ("saveOrigin", "1".into()),
      ("size", size.to_string()),
      ("small_flow_fname", forum.to_owned()),
      ("width", width.to_string()),
    ];
    let part = Part::file("chunk", chunk.to_vec(), "file", "application/octet-stream");
    last = c
      .call_multipart("/c/s/uploadPicture", params, part, WRITE)
      .await?;
  }
  let id = last
    .str("picId")
    .ok_or_else(|| Error::upstream("picture upload returned no picId"))?;
  let w = last
    .str("picInfo.originPic.width")
    .unwrap_or_else(|| width.to_string());
  let h = last
    .str("picInfo.originPic.height")
    .unwrap_or_else(|| height.to_string());
  Ok(format!("#(pic,{id},{w},{h})"))
}
