//! Forums (吧): a forum's threads and details, following a forum, and the
//! daily sign-in (签到).

use social_core::{
  Action, Collection, Error, ErrorCode, Page, PageReq, Post, Result, Value, ValueExt, json,
};

use crate::api::{Api, Params, WRITE};
use crate::refs::forum_url;
use crate::{parse, refs, users};

/// Orders of a forum's thread list.
pub const SORTS: &[&str] = &["reply", "new", "good"];

/// A forum page (`frs`): the forum and one page of its threads.
async fn frs(api: &Api, name: &str, sort: &str, pn: u64, rn: usize) -> Result<Value> {
  let mut params: Params = vec![
    ("kw", name.to_owned()),
    ("pn", pn.to_string()),
    ("rn", rn.to_string()),
    ("sort_type", if sort == "new" { "1" } else { "0" }.into()),
    ("q_type", "2".into()),
    ("st_type", "tb_forumlist".into()),
    ("with_group", "0".into()),
    ("scr_dip", "3.0".into()),
    ("scr_h", "2400".into()),
    ("scr_w", "1080".into()),
  ];
  if sort == "good" {
    params.push(("is_good", "1".into()));
    params.push(("cid", "0".into()));
  }
  let v = api.call("/c/f/frs/page", params).await?;
  // `forum.is_exists` says 0 for the newest-first order; the id tells.
  if v.str("forum.id").filter(|id| id != "0").is_none() {
    return Err(Error::not_found(format!("no Tieba forum named {name}")));
  }
  Ok(v)
}

/// One page of a forum's threads, by last reply, creation or 精品 only.
pub async fn threads(api: &Api, arg: &str, sort: &str, req: &PageReq) -> Result<Page<Post>> {
  let name = refs::forum(arg)?;
  let pn = req.number_or(1);
  let v = frs(api, &name, sort, pn, req.size_within(30)).await?;
  let users = parse::users(v.list("user_list"));
  let forum = v.str("forum.name").unwrap_or(name);
  let items: Vec<Post> = v
    .list("thread_list")
    .iter()
    .map(|t| {
      let mut post = parse::thread(t, &users);
      post
        .extra
        .entry("forum".into())
        .or_insert_with(|| forum.clone().into());
      post
    })
    .collect();
  let more = v.str("page.has_more").as_deref() == Some("1") && !items.is_empty();
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}

/// A forum's details: members, threads, posts, category, slogan.
pub async fn info(api: &Api, arg: &str) -> Result<Collection> {
  let name = refs::forum(arg)?;
  let v = frs(api, &name, "reply", 1, 1).await?;
  let mut forum = parse::forum(v.at("forum"))
    .ok_or_else(|| Error::upstream(format!("no details for forum {name}")))?;
  let managers: Vec<String> = v
    .list("forum.managers")
    .iter()
    .filter_map(|m| m.first_str(&["show_name", "name"]))
    .collect();
  if !managers.is_empty() {
    forum.extra.insert("managers".into(), managers.into());
  }
  Ok(forum)
}

/// Id and canonical name of a forum, which writes need.
pub async fn fid(api: &Api, arg: &str) -> Result<(String, String)> {
  let name = refs::forum(arg)?;
  let v = frs(api, &name, "reply", 1, 1).await?;
  let fid = v
    .str("forum.id")
    .ok_or_else(|| Error::upstream(format!("no id for forum {name}")))?;
  Ok((fid, v.str("forum.name").unwrap_or(name)))
}

/// Follow (关注) a forum, or leave it.
pub async fn follow_forum(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let (fid, kw) = fid(api, arg).await?;
  let tbs = api.tbs().await?;
  api.write_pause().await;
  let params: Params = vec![("fid", fid), ("kw", kw.clone()), ("tbs", tbs)];
  let path = if undo {
    "/c/c/forum/unlike"
  } else {
    "/c/c/forum/like"
  };
  api.call(path, params).await?;
  let name = if undo {
    "unfollow-forum"
  } else {
    "follow-forum"
  };
  Ok(Action::done(name, &kw).with_url(forum_url(&kw)))
}

/// Sign in (签到) to one forum: the result row of `sign`.
pub async fn sign(api: &Api, arg: &str) -> Result<Value> {
  api.require_login()?;
  let (fid, kw) = fid(api, arg).await?;
  let tbs = api.tbs().await?;
  api.write_pause().await;
  let params: Params = vec![("kw", kw.clone()), ("fid", fid), ("tbs", tbs)];
  let v = api.call_raw("/c/c/forum/sign", params, WRITE).await?;
  let code = v.str("error_code").unwrap_or_else(|| "0".into());
  let msg = v
    .first_str(&["error_msg", "error.usermsg"])
    .unwrap_or_default();
  // 160002: already signed today.
  let ok = code == "0" || code == "160002";
  if !ok && code != "340006" {
    // Hard failures (session, captcha, rate limit) stop a batch.
    let error = api.error(&code, &msg);
    if matches!(
      error.code,
      ErrorCode::NotAuthenticated | ErrorCode::VerificationRequired | ErrorCode::RateLimited
    ) {
      return Err(error);
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
pub async fn sign_all(api: &Api) -> Result<Vec<Value>> {
  let all = social_core::paging::collect(usize::MAX, None, async |r| {
    users::forums(api, None, &r).await
  })
  .await?;
  let mut rows = Vec::new();
  for f in all.items {
    rows.push(sign(api, &f.name).await?);
  }
  Ok(rows)
}
