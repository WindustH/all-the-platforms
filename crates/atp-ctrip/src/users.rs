//! Accounts and community users. `whoami` asks the member service
//! (`15201/getMemberSummaryInfo`, from `trains.ctrip.com` as the site does);
//! `user` reads a community profile (`16486/json/getUserPage`) and
//! `user-posts` its notes, reviews and answers (`20725/json/searchMyMixList`,
//! cursor `pageKey`).

use atp_core::{Error, Page, PageReq, Post, Result, User, ValueExt, json};

use crate::api::{Api, Site};
use crate::parse;
use crate::refs;

/// Entries a page of a user's posts holds.
const STEP: u64 = 10;

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let body = json!({ "channel": "Online", "clientVersion": "99.99" });
  let v = api
    .soa("15201/getMemberSummaryInfo", Site::Trains, body)
    .await?;
  let id = v.str("uid").ok_or_else(|| {
    Error::auth("Ctrip did not name the session's account").with_hint(api.ctx.login_hint())
  })?;
  let mut user = User {
    name: v.str("userName").unwrap_or_else(|| id.clone()),
    id,
    ..User::default()
  };
  parse::put(&mut user.extra, "grade", v.get("grade").cloned());
  parse::put(&mut user.extra, "svip", v.get("svip").cloned());
  user.raw = Some(v);
  Ok(user)
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = refs::user(arg)?;
  let body = json!({ "clientAuth": id, "contentType": "json" });
  let v = api
    .soa("16486/json/getUserPage", Site::Mobile, body)
    .await
    .map_err(|e| missing(e, &id))?;
  parse::user(&v).ok_or_else(|| Error::not_found(format!("no Ctrip user {id}")))
}

pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = refs::user(arg)?;
  let body = json!({
    "clientAuth": id,
    "type": 0,
    "pageKey": page.cursor.clone().unwrap_or_default(),
    "pagePara": { "pageSize": STEP, "sorttype": 1 },
    "source": "mylist",
    "needGuidePublish": false,
    "needFastData": true,
    "isSeparateDynamic": true
  });
  let v = api
    .soa("20725/json/searchMyMixList", Site::Mobile, body)
    .await
    .map_err(|e| missing(e, &id))?;
  Ok(parse::mix_page(&v, "resultList"))
}

/// Ctrip's answer for an unknown id is a request failure.
fn missing(e: Error, id: &str) -> Error {
  if e.code == atp_core::ErrorCode::UpstreamError {
    Error::not_found(format!("no Ctrip user {id} ({})", e.message))
  } else {
    e
  }
}
