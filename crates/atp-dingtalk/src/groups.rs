//! Groups the app can look into: scene groups (场景群, created from a group
//! template) by `openConversationId`. Ordinary groups have no member or
//! info API for robots.

use atp_core::{Collection, Error, Page, PageReq, Result, User, Value, ValueExt, json};

use crate::api::{Api, OapiBody};
use crate::refs::{self, Target};

fn cid(arg: &str) -> Result<String> {
  match refs::target(arg)? {
    Target::Group(cid) => Ok(cid),
    _ => Err(Error::input(format!(
      "not a group: {arg}; give its openConversationId (cid…)"
    ))),
  }
}

pub async fn info(api: &Api, arg: &str) -> Result<Collection> {
  let id = cid(arg)?;
  let v = api
    .oapi(
      "/topapi/im/chat/scenegroup/get",
      &[],
      OapiBody::Json(json!({ "open_conversation_id": id })),
    )
    .await?;
  let r = v.at("result");
  let mut c = Collection {
    id: id.clone(),
    kind: "group".into(),
    name: r.str("title").unwrap_or_default(),
    owner: r.str("owner_staff_id").map(|id| User {
      id,
      ..User::default()
    }),
    raw: Some(r.clone()),
    ..Collection::default()
  };
  for key in ["chatid", "template_id", "icon", "group_url"] {
    if let Some(v) = r.str(key) {
      c.extra.insert(key.into(), Value::from(v));
    }
  }
  Ok(c)
}

pub async fn members(api: &Api, arg: &str, req: &PageReq) -> Result<Page<User>> {
  let id = cid(arg)?;
  let v = api
    .oapi(
      "/topapi/im/chat/scenegroup/member/get",
      &[],
      OapiBody::Json(json!({
        "open_conversation_id": id,
        "cursor": req.cursor.clone().unwrap_or_else(|| "0".into()),
        "size": req.size_within(1000),
      })),
    )
    .await
    .map_err(|e| {
      if e.code == atp_core::ErrorCode::UpstreamError {
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
