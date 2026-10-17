//! Follow groups (关注分组) as the follow-group page and its group dialog
//! manage them: listing them and their people, creating, renaming and
//! deleting them, and putting people in or taking them out.

use atp_core::bulk;
use atp_core::{Action, Collection, Error, Page, PageReq, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::parse::{self, Own};
use crate::refs::user_url;
use crate::users;

/// Your follow groups, the ones Weibo keeps (`extra.mode` `system`)
/// among them.
pub async fn list(api: &Api) -> Result<Page<Collection>> {
  Ok(Page::last(all(api).await?))
}

async fn all(api: &Api) -> Result<Vec<Collection>> {
  api.require_login()?;
  let v = api
    .get("/ajax/profile/getGroups", &[("showBilateral", "1")])
    .await?;
  Ok(
    v.list("data.lists")
      .iter()
      .filter_map(parse::group)
      .collect(),
  )
}

/// A group from its id or exact name.
async fn find(api: &Api, group: &str) -> Result<Collection> {
  all(api)
    .await?
    .into_iter()
    .find(|g| g.id == group || g.name == group)
    .ok_or_else(|| Error::not_found(format!("no follow group {group}")))
}

/// The people in one of your follow groups.
pub async fn members(api: &Api, group: &str, page: &PageReq) -> Result<Page<User>> {
  let id = find(api, group).await?.id;
  let no = page.number_or(1);
  let no_text = no.to_string();
  let v = api
    .get(
      "/ajax/profile/getGroupMembers",
      &[("list_id", id.as_str()), ("page", &no_text)],
    )
    .await?;
  let users: Vec<User> = v
    .list("data.users")
    .iter()
    .filter_map(|u| parse::relation(u, Own::Following))
    .collect();
  let more = !users.is_empty() && v.i64("data.next_cursor").is_some_and(|n| n != 0);
  Ok(Page::new(users, more.then(|| (no + 1).to_string())))
}

/// Create a private group.
pub async fn create(api: &Api, name: &str) -> Result<Action> {
  api.require_login()?;
  api.write_pause().await;
  let v = api
    .post_json(
      "/ajax/profile/createGroup",
      &json!({"name": name, "isOpen": false}),
    )
    .await?;
  let action = Action::done("follow-group-create", name);
  Ok(match v.first_str(&["data.idstr", "data.id"]) {
    Some(id) => action.with_id(id),
    None => action,
  })
}

/// Rename a group; whether it is public stays as it was.
pub async fn rename(api: &Api, group: &str, name: &str) -> Result<Action> {
  let group = find(api, group).await?;
  let public = group.extra.get("mode") == Some(&Value::from("public"));
  api.write_pause().await;
  api
    .post_json(
      "/ajax/profile/updateGroup",
      &json!({"name": name, "isOpen": public, "list_id": group.id}),
    )
    .await?;
  Ok(Action::done("follow-group-rename", group.id).with_message(name.to_owned()))
}

/// Delete a group; its people stay followed.
pub async fn delete(api: &Api, group: &str) -> Result<Action> {
  let id = find(api, group).await?.id;
  api.write_pause().await;
  api
    .post_json("/ajax/profile/destroyGroup", &json!({"list_id": id}))
    .await?;
  Ok(Action::done("follow-group-delete", id))
}

/// Put users into a group, keeping their other groups.
pub async fn add(api: &Api, group: &str, inputs: &[String]) -> Result<Vec<Action>> {
  regroup(api, group, inputs, "follow-group-add", true).await
}

/// Take users out of a group; their other groups stay.
pub async fn remove(api: &Api, group: &str, inputs: &[String]) -> Result<Vec<Action>> {
  regroup(api, group, inputs, "follow-group-remove", false).await
}

/// `setGroup` sets a user's groups all at once (`list_ids`, with the ones
/// before in `origin_list_ids`), so each user's groups are read first
/// (`getGroupList`) and sent back with `group` added or taken out.
async fn regroup(
  api: &Api,
  group: &str,
  inputs: &[String],
  action: &str,
  add: bool,
) -> Result<Vec<Action>> {
  let id = find(api, group).await?.id;
  let mut uids = Vec::with_capacity(inputs.len());
  for input in inputs {
    uids.push(users::uid(api, input).await?);
  }
  bulk::batches(action, &uids, 1, String::clone, async |one| {
    let uid = &one[0];
    let before = of_user(api, uid).await?;
    let mut after = before.clone();
    match (add, after.iter().position(|g| *g == id)) {
      (true, None) => after.push(id.clone()),
      (false, Some(at)) => {
        after.remove(at);
      }
      (true, Some(_)) | (false, None) => {
        let already = if add { "already in" } else { "not in" };
        let done = Action::done(action, uid).with_url(user_url(uid));
        return Ok(vec![done.with_message(format!("{already} group {id}"))]);
      }
    }
    api.write_pause().await;
    let v = api
      .post_json(
        "/ajax/profile/setGroup",
        &json!({
          "uids": uid,
          "list_ids": after.join(","),
          "list_name": "",
          "origin_list_ids": before.join(","),
        }),
      )
      .await?;
    if v.bool("data.result") == Some(false) {
      return Err(Error::upstream(format!(
        "Weibo did not change the groups of {uid}"
      )));
    }
    let done = Action::done(action, uid).with_url(user_url(uid));
    Ok(vec![
      done.with_message(format!("groups {}", after.join(","))),
    ])
  })
  .await
}

/// The ids of the groups a user you follow is in.
async fn of_user(api: &Api, uid: &str) -> Result<Vec<String>> {
  let v = api
    .get("/ajax/profile/getGroupList", &[("uid", uid)])
    .await?;
  Ok(
    v.list("data")
      .iter()
      .filter_map(|g| g.first_str(&["idstr", "id"]))
      .collect(),
  )
}
