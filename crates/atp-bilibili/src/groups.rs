//! Follow groups (关注分组): listing them and their people, creating,
//! renaming and deleting them, and putting people in or taking them out.
//! Group `-10` is the special-follow list (特别关注), `0` the default group
//! of everyone in no other.

use std::collections::BTreeSet;

use atp_core::bulk;
use atp_core::{Action, Collection, Error, Page, PageReq, Result, User, ValueExt};

use crate::api::Api;
use crate::paging::Pn;
use crate::parse::{self, Rows};
use crate::refs::space_url;
use crate::users;

const TAGS: &str = "https://api.bilibili.com/x/relation/tags";
const TAG: &str = "https://api.bilibili.com/x/relation/tag";
const TAG_USER: &str = "https://api.bilibili.com/x/relation/tag/user";
const TAG_CREATE: &str = "https://api.bilibili.com/x/relation/tag/create";
const TAG_UPDATE: &str = "https://api.bilibili.com/x/relation/tag/update";
const TAG_DEL: &str = "https://api.bilibili.com/x/relation/tag/del";
const ADD_USERS: &str = "https://api.bilibili.com/x/relation/tags/addUsers";

/// The default group: everyone in no other group.
const DEFAULT: &str = "0";

/// The web client tags these writes with its device description.
const DEVICE: &str = r#"{"platform":"web","device":"pc","spmid":"333.1387"}"#;

/// Your follow groups, with how many people each holds.
pub async fn list(api: &Api) -> Result<Page<Collection>> {
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
async fn id(api: &Api, group: &str) -> Result<String> {
  if group.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
    return Ok(group.to_owned());
  }
  list(api)
    .await?
    .items
    .into_iter()
    .find(|g| g.name == group)
    .map(|g| g.id)
    .ok_or_else(|| Error::not_found(format!("no follow group named {group}")))
}

/// The people in one of your follow groups, in follow order or (`sort`
/// `visited`) most visited first.
pub async fn members(
  api: &Api,
  group: &str,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<User>> {
  let id = id(api, group).await?;
  let pn = Pn::of(page, 50);
  let mut call = api
    .get(TAG)
    .arg("tagid", &id)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps);
  match sort {
    None | Some("recent") => {}
    Some("visited") => call = call.arg("order_type", "attention"),
    Some(other) => {
      return Err(Error::input(format!(
        "unknown --sort {other} (recent or visited)"
      )));
    }
  }
  let data = call.send().await?;
  let users: Vec<User> = data
    .as_array()
    .into_iter()
    .flatten()
    .map(|v| parse::relation(v, Rows::Others))
    .collect();
  let full = users.len() >= pn.ps;
  Ok(pn.page(users, full))
}

pub async fn create(api: &Api, name: &str) -> Result<Action> {
  let data = api
    .post(TAG_CREATE)
    .query("x-bili-device-req-json", DEVICE)
    .arg("tag", name)
    .send()
    .await?;
  let id = data.str("tagid").unwrap_or_default();
  Ok(Action::done("follow-group-create", name).with_id(id))
}

pub async fn rename(api: &Api, group: &str, name: &str) -> Result<Action> {
  let id = id(api, group).await?;
  api
    .post(TAG_UPDATE)
    .query("x-bili-device-req-json", DEVICE)
    .arg("tagid", &id)
    .arg("name", name)
    .send()
    .await?;
  Ok(Action::done("follow-group-rename", id).with_message(name.to_owned()))
}

/// Delete a group; its people move to the default group.
pub async fn delete(api: &Api, group: &str) -> Result<Action> {
  let id = id(api, group).await?;
  api
    .post(TAG_DEL)
    .query("x-bili-device-req-json", DEVICE)
    .arg("tagid", &id)
    .send()
    .await?;
  Ok(Action::done("follow-group-delete", id))
}

/// Put users into a group, keeping their other groups.
pub async fn add(api: &Api, group: &str, inputs: &[String]) -> Result<Vec<Action>> {
  regroup(api, group, inputs, "follow-group-add", true).await
}

/// Take users out of a group; with no group left they are in the default one.
pub async fn remove(api: &Api, group: &str, inputs: &[String]) -> Result<Vec<Action>> {
  regroup(api, group, inputs, "follow-group-remove", false).await
}

/// `tags/addUsers` replaces a user's groups, so each user's groups are
/// read first (`tag/user`) and sent back with `group` added or taken out.
async fn regroup(
  api: &Api,
  group: &str,
  inputs: &[String],
  action: &str,
  add: bool,
) -> Result<Vec<Action>> {
  let id = id(api, group).await?;
  if id == DEFAULT {
    return Err(Error::input(
      "the default group (0) holds whoever is in no other group; add them to another one",
    ));
  }
  let mids = users::mids(api, inputs).await?;
  bulk::batches(action, &mids, 1, String::clone, async |one| {
    let mid = &one[0];
    let mut groups = of_user(api, mid).await?;
    let changed = if add {
      groups.insert(id.clone())
    } else {
      groups.remove(&id)
    };
    let done = Action::done(action, mid).with_url(space_url(mid));
    if !changed {
      let already = if add { "already in" } else { "not in" };
      return Ok(vec![done.with_message(format!("{already} group {id}"))]);
    }
    let tags = if groups.is_empty() {
      DEFAULT.to_owned()
    } else {
      groups.into_iter().collect::<Vec<_>>().join(",")
    };
    api
      .post(ADD_USERS)
      .query("x-bili-device-req-json", DEVICE)
      .arg("fids", mid)
      .arg("tagids", &tags)
      .send()
      .await?;
    Ok(vec![done.with_message(format!("groups {tags}"))])
  })
  .await
}

/// The ids of the groups a user you follow is in (`-10` for a special
/// follow); empty for the default group.
async fn of_user(api: &Api, mid: &str) -> Result<BTreeSet<String>> {
  let data = api.get(TAG_USER).arg("fid", mid).send().await?;
  Ok(
    data
      .as_object()
      .map(|groups| groups.keys().filter(|k| *k != DEFAULT).cloned().collect())
      .unwrap_or_default(),
  )
}
