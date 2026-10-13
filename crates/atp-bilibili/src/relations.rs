//! Relations beyond follow / unfollow: follow groups (分组), special
//! follows, the blacklist and followings shared with another user.

use atp_core::text::from_secs;
use atp_core::{Action, Collection, Error, Page, PageReq, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::paging::Pn;
use crate::refs::space_url;
use crate::{parse, users};

const TAGS: &str = "https://api.bilibili.com/x/relation/tags";
const TAG: &str = "https://api.bilibili.com/x/relation/tag";
const TAG_CREATE: &str = "https://api.bilibili.com/x/relation/tag/create";
const TAG_UPDATE: &str = "https://api.bilibili.com/x/relation/tag/update";
const TAG_DEL: &str = "https://api.bilibili.com/x/relation/tag/del";
const ADD_USERS: &str = "https://api.bilibili.com/x/relation/tags/addUsers";
const SPECIAL_ADD: &str = "https://api.bilibili.com/x/relation/tag/special/add";
const SPECIAL_DEL: &str = "https://api.bilibili.com/x/relation/tag/special/del";
const BLACKS: &str = "https://api.bilibili.com/x/relation/blacks";
const MODIFY: &str = "https://api.bilibili.com/x/relation/modify";
const SAME: &str = "https://api.bilibili.com/x/relation/same/followings";

/// The web client tags these writes with its device description.
const DEVICE: &str = r#"{"platform":"web","device":"pc","spmid":"333.1387"}"#;

/// Your follow groups, with how many people each holds.
pub async fn groups(api: &Api) -> Result<Page<Collection>> {
  api.require_login()?;
  let data = api.get(TAGS).send().await?;
  let items = data
    .as_array()
    .into_iter()
    .flatten()
    .map(|t| Collection {
      id: t.str("tagid").unwrap_or_default(),
      kind: "group".into(),
      name: t.str("name").unwrap_or_default(),
      description: t.str("tip").filter(|d| !d.is_empty()),
      items: t.count("count"),
      raw: Some(t.clone()),
      ..Collection::default()
    })
    .collect();
  Ok(Page::last(items))
}

/// A group id from its id or exact name.
async fn group_id(api: &Api, group: &str) -> Result<String> {
  if group.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
    return Ok(group.to_owned());
  }
  groups(api)
    .await?
    .items
    .into_iter()
    .find(|g| g.name == group)
    .map(|g| g.id)
    .ok_or_else(|| Error::not_found(format!("no follow group named {group}")))
}

/// The people in one of your follow groups.
pub async fn members(api: &Api, group: &str, page: &PageReq) -> Result<Page<User>> {
  let id = group_id(api, group).await?;
  let pn = Pn::of(page, 50);
  let data = api
    .get(TAG)
    .arg("tagid", &id)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .send()
    .await?;
  let users: Vec<User> = data.as_array().into_iter().flatten().map(person).collect();
  let full = users.len() >= pn.ps;
  Ok(pn.page(users, full))
}

fn person(v: &Value) -> User {
  let mut u = parse::user(v);
  if let Some(at) = v.i64("mtime").and_then(from_secs) {
    u.extra.insert("followed_at".into(), json!(at));
  }
  u
}

pub async fn create_group(api: &Api, name: &str) -> Result<Action> {
  let data = api
    .post(TAG_CREATE)
    .query("x-bili-device-req-json", DEVICE)
    .arg("tag", name)
    .send()
    .await?;
  let id = data.str("tagid").unwrap_or_default();
  Ok(Action::done("group-create", name).with_id(id))
}

pub async fn rename_group(api: &Api, group: &str, name: &str) -> Result<Action> {
  let id = group_id(api, group).await?;
  api
    .post(TAG_UPDATE)
    .query("x-bili-device-req-json", DEVICE)
    .arg("tagid", &id)
    .arg("name", name)
    .send()
    .await?;
  Ok(Action::done("group-rename", id).with_message(name.to_owned()))
}

pub async fn delete_group(api: &Api, group: &str) -> Result<Action> {
  let id = group_id(api, group).await?;
  api
    .post(TAG_DEL)
    .query("x-bili-device-req-json", DEVICE)
    .arg("tagid", &id)
    .send()
    .await?;
  Ok(Action::done("group-delete", id))
}

/// Put users into groups (replacing their current groups); no groups = 默认分组.
pub async fn assign(api: &Api, users: &[String], groups: &[String]) -> Result<Action> {
  let mut mids = Vec::new();
  for u in users {
    mids.push(users::mid(api, u).await?);
  }
  let mut ids = Vec::new();
  for g in groups {
    ids.push(group_id(api, g).await?);
  }
  let tags = if ids.is_empty() {
    "0".to_owned()
  } else {
    ids.join(",")
  };
  api
    .post(ADD_USERS)
    .query("x-bili-device-req-json", DEVICE)
    .arg("fids", mids.join(","))
    .arg("tagids", &tags)
    .send()
    .await?;
  Ok(Action::done("group-assign", mids.join(",")).with_message(format!("groups {tags}")))
}

/// Mark (or with `undo` unmark) a user as a special follow (特别关注).
pub async fn special(api: &Api, input: &str, undo: bool) -> Result<Action> {
  let mid = users::mid(api, input).await?;
  api
    .post(if undo { SPECIAL_DEL } else { SPECIAL_ADD })
    .arg("fid", &mid)
    .send()
    .await?;
  let action = if undo { "unspecial" } else { "special" };
  Ok(Action::done(action, &mid).with_url(space_url(&mid)))
}

pub async fn blacklist(api: &Api, page: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let pn = Pn::of(page, 50);
  let data = api
    .get(BLACKS)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .send()
    .await?;
  let users = data
    .list("list")
    .iter()
    .map(|v| {
      let mut u = parse::user(v);
      if let Some(at) = v.i64("mtime").and_then(from_secs) {
        u.extra.insert("blocked_at".into(), json!(at));
      }
      u
    })
    .collect();
  Ok(pn.page(users, pn.before(data.u64("total"))))
}

/// Block (or with `undo` unblock) a user.
pub async fn block(api: &Api, input: &str, undo: bool) -> Result<Action> {
  let mid = users::mid(api, input).await?;
  api
    .post(MODIFY)
    .arg("fid", &mid)
    .arg("act", if undo { 6 } else { 5 })
    .arg("re_src", 11)
    .send()
    .await?;
  let action = if undo { "unblock" } else { "block" };
  Ok(Action::done(action, &mid).with_url(space_url(&mid)))
}

/// People both you and `input` follow.
pub async fn shared(api: &Api, input: &str, page: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let mid = users::mid(api, input).await?;
  let pn = Pn::of(page, 50);
  let data = api
    .get(SAME)
    .arg("vmid", &mid)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .send()
    .await?;
  let users = data.list("list").iter().map(person).collect();
  Ok(pn.page(users, pn.before(data.u64("total"))))
}
