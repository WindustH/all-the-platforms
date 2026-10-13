//! Groups the app can look into: scene groups (场景群, created from a group
//! template) by `openConversationId`. Ordinary groups have no member or
//! info API for robots.

use atp_core::{Collection, Error, ErrorCode, Page, PageReq, Result, User, ValueExt, json};

use crate::api::{Api, OapiBody};
use crate::parse;
use crate::refs::{self, Target};

/// The `openConversationId` a chat argument names.
fn conversation(arg: &str) -> Result<String> {
  match refs::target(arg)? {
    Target::Group(id) => Ok(id),
    _ => Err(Error::input(format!(
      "not a group: {arg}; give its openConversationId (cid…)"
    ))),
  }
}

/// A scene group's details.
pub async fn group(api: &Api, arg: &str) -> Result<Collection> {
  let id = conversation(arg)?;
  let body = OapiBody::Json(json!({ "open_conversation_id": id }));
  let v = api
    .oapi("/topapi/im/chat/scenegroup/get", &[], body)
    .await?;
  Ok(parse::scene_group(&id, v.at("result")))
}

pub async fn members(api: &Api, arg: &str, req: &PageReq) -> Result<Page<User>> {
  let id = conversation(arg)?;
  let body = OapiBody::Json(json!({
    "open_conversation_id": id,
    "cursor": req.cursor.clone().unwrap_or_else(|| "0".into()),
    "size": req.size_within(1000),
  }));
  let v = api
    .oapi("/topapi/im/chat/scenegroup/member/get", &[], body)
    .await
    .map_err(|e| {
      if e.code == ErrorCode::UpstreamError {
        e.with_hint("DingTalk lists members of scene groups (场景群) only")
      } else {
        e
      }
    })?;
  let r = v.at("result");
  let users = r
    .list("member_user_ids")
    .iter()
    .filter_map(|u| u.as_str())
    .map(|id| User {
      id: id.to_owned(),
      ..User::default()
    })
    .collect();
  let next = (r.bool("has_more") == Some(true))
    .then(|| r.str("next_cursor"))
    .flatten();
  Ok(Page::new(users, next))
}
