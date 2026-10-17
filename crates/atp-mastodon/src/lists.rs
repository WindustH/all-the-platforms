//! Lists as follow groups: lists of accounts you follow, each with its own
//! timeline (`/api/v1/lists`, `/api/v1/lists/:id/accounts` with
//! `account_ids[]`). Mastodon adds only accounts you follow.

use atp_core::bulk;
use atp_core::http::Method;
use atp_core::{Action, Collection, Error, Page, PageReq, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::{parse, users};

/// A list as a follow group; how replies show and whether it hides its
/// accounts from the home timeline in `extra`.
fn list(v: &Value, instance: &str) -> Option<Collection> {
  let id = v.str("id")?;
  let mut list = Collection {
    kind: "group".into(),
    name: v.str("title").unwrap_or_default(),
    url: Some(format!("https://{instance}/lists/{id}")),
    raw: Some(v.clone()),
    id,
    ..Collection::default()
  };
  if let Some(policy) = v.str("replies_policy") {
    list.extra.insert("replies_policy".into(), policy.into());
  }
  if let Some(exclusive) = v.bool("exclusive") {
    list.extra.insert("exclusive".into(), exclusive.into());
  }
  Some(list)
}

/// Your lists, all at once.
pub async fn groups(api: &Api) -> Result<Page<Collection>> {
  api.require()?;
  let v = api.get("v1/lists", Vec::new()).await?;
  let instance = api.instance();
  let rows = v.as_array().map(Vec::as_slice).unwrap_or_default();
  Ok(Page::last(
    rows.iter().filter_map(|l| list(l, &instance)).collect(),
  ))
}

fn list_id(arg: &str) -> Result<String> {
  let id = arg.trim().trim_end_matches('/');
  let id = id.rsplit('/').next().unwrap_or(id);
  if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
    return Err(Error::input(format!(
      "not a list: {arg} (expected its id, a /lists/ID link or #N after follow-groups)"
    )));
  }
  Ok(id.to_owned())
}

/// The accounts on a list.
pub async fn members(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  api.require()?;
  let path = format!("v1/lists/{}/accounts", list_id(arg)?);
  api.list(&path, Vec::new(), page, parse::user).await
}

pub async fn create(api: &Api, title: &str) -> Result<Action> {
  let v = api
    .write(Method::POST, "v1/lists", vec![("title", title.to_owned())])
    .await?;
  let mut action = Action::done("follow-group-create", title);
  if let Some(id) = v.str("id") {
    action = action
      .with_url(format!("https://{}/lists/{id}", api.instance()))
      .with_id(id);
  }
  Ok(action)
}

/// A new title; how replies show and whether the list is exclusive stay.
pub async fn rename(api: &Api, arg: &str, title: &str) -> Result<Action> {
  let id = list_id(arg)?;
  let path = format!("v1/lists/{id}");
  api
    .write(Method::PUT, &path, vec![("title", title.to_owned())])
    .await?;
  Ok(Action::done("follow-group-rename", &id).with_message(title.to_owned()))
}

/// Delete a list; its accounts stay followed.
pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let id = list_id(arg)?;
  api
    .write(Method::DELETE, &format!("v1/lists/{id}"), Vec::new())
    .await?;
  Ok(Action::done("follow-group-delete", &id))
}

/// Put accounts on a list (`undo`: take them off), one call each, so one
/// refused account (not followed, already on it) does not stop the rest.
pub async fn file(api: &Api, arg: &str, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  api.require()?;
  let list = list_id(arg)?;
  let (method, action) = if undo {
    (Method::DELETE, "follow-group-remove")
  } else {
    (Method::POST, "follow-group-add")
  };
  let path = format!("v1/lists/{list}/accounts");
  bulk::each(action, inputs, async |input| {
    let id = users::id(api, input).await?;
    let form = vec![("account_ids[]", id.clone())];
    api.write(method.clone(), &path, form).await?;
    Ok(Action::done(action, &id).with_message(format!("list {list}")))
  })
  .await
}
