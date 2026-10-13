//! Groups and friends, and group members. The lists come whole; paging is
//! local.

use atp_core::paging::slice;
use atp_core::{Collection, Error, Page, PageReq, Result, User, Value, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat, num};

/// Groups, then friends.
pub async fn list(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  let groups = api.call("get_group_list", json!({})).await?;
  let friends = api.call("get_friend_list", json!({})).await?;
  let all: Vec<Collection> = rows(&groups)
    .iter()
    .map(parse::group)
    .chain(rows(&friends).iter().map(parse::friend))
    .collect();
  Ok(slice(all, page))
}

pub async fn members(api: &Api, chat: &str, page: &PageReq) -> Result<Page<User>> {
  let Chat::Group(group) = refs::chat(chat)? else {
    return Err(Error::input(
      "members are listed for groups (group:<number>)",
    ));
  };
  let v = api
    .call("get_group_member_list", json!({"group_id": num(&group)}))
    .await?;
  Ok(slice(rows(&v).iter().map(parse::user).collect(), page))
}

fn rows(v: &Value) -> Vec<Value> {
  v.as_array().cloned().unwrap_or_default()
}
