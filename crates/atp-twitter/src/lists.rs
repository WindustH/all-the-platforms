//! Lists: one list, its members and followers, the lists a user is on, and
//! managing your own (create, edit, delete, add / remove members, follow).

use atp_core::{Action, Collection, Error, Page, PageReq, Result, User, ValueExt, json};

use crate::api::Api;
use crate::graphql::{
  CREATE_LIST, DELETE_LIST, LIST_ADD, LIST_BY_ID, LIST_MEMBERS, LIST_MEMBERSHIPS, LIST_REMOVE,
  LIST_SUBSCRIBE, LIST_SUBSCRIBERS, LIST_UNSUBSCRIBE, UPDATE_LIST,
};
use crate::parse;
use crate::refs::{self, list_url};
use crate::timeline::{self, Timeline, vars, with};
use crate::users;

pub async fn info(api: &Api, arg: &str) -> Result<Collection> {
  let id = refs::list_id(arg)?;
  api.require_login()?;
  let data = api.graphql(&LIST_BY_ID, json!({ "listId": id })).await?;
  parse::list(data.at("data.list")).ok_or_else(|| Error::not_found(format!("no list {id}")))
}

/// Members (`followers`: false) or followers of a list.
pub async fn people(api: &Api, arg: &str, followers: bool, req: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let id = refs::list_id(arg)?;
  let (op, path) = if followers {
    (
      &LIST_SUBSCRIBERS,
      "data.list.subscribers_timeline.timeline.instructions",
    )
  } else {
    (
      &LIST_MEMBERS,
      "data.list.members_timeline.timeline.instructions",
    )
  };
  let variables = with(
    vars(req),
    json!({ "listId": id, "withSafetyModeUserFields": true }),
  );
  timeline::page(api, op, variables, &[path], req, timeline::users).await
}

/// Lists a user (you when absent) is a member of.
pub async fn memberships(api: &Api, arg: Option<&str>, req: &PageReq) -> Result<Page<Collection>> {
  let id = users::user_id_or_self(api, arg).await?;
  let variables = with(
    vars(req),
    json!({ "userId": id, "isListMembershipShown": true, "isListMemberTargetUserId": id }),
  );
  let paths = &[
    "data.user.result.timeline.timeline.instructions",
    "data.user.result.timeline_v2.timeline.instructions",
  ];
  timeline::page(
    api,
    &LIST_MEMBERSHIPS,
    variables,
    paths,
    req,
    |tl: &Timeline| {
      tl.items()
        .filter_map(|(_, item)| parse::list(item.at("list")))
        .collect()
    },
  )
  .await
}

pub async fn create(
  api: &Api,
  name: &str,
  description: Option<&str>,
  private: bool,
) -> Result<Action> {
  let variables = json!({
    "name": name,
    "description": description.unwrap_or_default(),
    "isPrivate": private,
  });
  let data = api.mutate(&CREATE_LIST, variables).await?;
  let id = data
    .str("data.list.id_str")
    .ok_or_else(|| Error::upstream("X did not create the list"))?;
  Ok(
    Action::done("create-list", name)
      .with_id(&id)
      .with_url(list_url(&id)),
  )
}

pub async fn update(
  api: &Api,
  arg: &str,
  name: Option<&str>,
  description: Option<&str>,
  private: Option<bool>,
) -> Result<Action> {
  let id = refs::list_id(arg)?;
  let mut variables = json!({ "listId": id });
  if let Some(n) = name {
    variables["name"] = n.into();
  }
  if let Some(d) = description {
    variables["description"] = d.into();
  }
  if let Some(p) = private {
    variables["isPrivate"] = p.into();
  }
  api.mutate(&UPDATE_LIST, variables).await?;
  Ok(Action::done("update-list", &id).with_url(list_url(&id)))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let id = refs::list_id(arg)?;
  api.mutate(&DELETE_LIST, json!({ "listId": id })).await?;
  Ok(Action::done("delete-list", id))
}

/// Add a user to a list, or remove them with `undo`.
pub async fn member(api: &Api, list: &str, user: &str, undo: bool) -> Result<Action> {
  let id = refs::list_id(list)?;
  let user_id = users::user_id(api, user).await?;
  let (op, name) = if undo {
    (&LIST_REMOVE, "list-remove")
  } else {
    (&LIST_ADD, "list-add")
  };
  api
    .mutate(op, json!({ "listId": id, "userId": user_id }))
    .await?;
  Ok(
    Action::done(name, &user_id)
      .with_url(list_url(&id))
      .with_message(format!("list {id}")),
  )
}

/// Follow (subscribe to) a list, or unfollow it with `undo`.
pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::list_id(arg)?;
  let (op, name) = if undo {
    (&LIST_UNSUBSCRIBE, "unfollow-list")
  } else {
    (&LIST_SUBSCRIBE, "follow-list")
  };
  api.mutate(op, json!({ "listId": id })).await?;
  Ok(Action::done(name, &id).with_url(list_url(&id)))
}
