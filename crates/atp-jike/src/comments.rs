//! Comments: a post's comments (`/1.0/comments/listPrimary`, by likes or
//! by time, each with the hot replies Jike picks), the replies of one
//! comment (`/1.0/comments/list`), commenting and replying
//! (`/1.0/comments/add`) and deleting your comment (`/1.0/comments/remove`).

use atp_core::{Action, Comment, Error, Page, PageReq, Reply, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs::Kind;
use crate::{posts, uploads};

pub const SORTS: &[&str] = &["hot", "latest"];

fn order(sort: Option<&str>) -> &'static str {
  match sort {
    Some("latest") => "TIME",
    _ => "LIKES",
  }
}

fn page(v: &Value, next: Option<String>) -> Page<Comment> {
  Page::new(
    v.list("data").iter().map(crate::parse::comment).collect(),
    next,
  )
}

pub async fn list(
  api: &Api,
  arg: &str,
  sort: Option<&str>,
  req: &PageReq,
) -> Result<Page<Comment>> {
  let (id, kind) = posts::resolve(api, arg).await?;
  let body = json!({"targetId": id, "targetType": kind.api_type(), "order": order(sort)});
  let (v, next) = api.list("/1.0/comments/listPrimary", body, req).await?;
  Ok(page(&v, next))
}

/// One comment (`targetType` is the post's type).
async fn get(api: &Api, id: &str, kind: Kind) -> Result<Value> {
  let params = [
    ("id", id.to_owned()),
    ("targetType", kind.api_type().to_owned()),
  ];
  let v = api.get("/1.0/comments/get", &params).await?;
  let data = v.at("data");
  if data.str("id").is_none() {
    return Err(Error::not_found(format!("Jike has no comment {id}")));
  }
  Ok(data.clone())
}

pub async fn replies(api: &Api, arg: &str, comment: &str, req: &PageReq) -> Result<Page<Comment>> {
  let (id, kind) = posts::resolve(api, arg).await?;
  let parent = get(api, comment.trim(), kind).await?;
  let thread = parent
    .str("threadId")
    .unwrap_or_else(|| comment.trim().to_owned());
  let body = json!({
    "targetId": id,
    "targetType": kind.api_type(),
    "primaryCommentId": thread,
    "level": parent.u64("level").unwrap_or(1),
    "order": "LIKES",
  });
  let (v, next) = api.list("/1.0/comments/list", body, req).await?;
  Ok(page(&v, next))
}

/// Comment on a post, or answer one of its comments; one picture at most.
pub async fn add(api: &Api, arg: &str, reply: &Reply) -> Result<Action> {
  api.require_login()?;
  if reply.images.len() > 1 {
    return Err(Error::input("a Jike comment carries one picture at most"));
  }
  let (id, kind) = posts::resolve(api, arg).await?;
  let mut keys = Vec::new();
  for path in &reply.images {
    keys.push(uploads::picture(api, path).await?);
  }
  let mut body = json!({
    "targetType": kind.api_type(),
    "targetId": id,
    "content": reply.text,
    "syncToPersonalUpdates": false,
    "pictureKeys": keys,
    "force": false,
  });
  if let Some(answered) = &reply.reply_to {
    let parent = get(api, answered.trim(), kind).await?;
    body["threadId"] = json!(parent.str("threadId"));
    body["replyToCommentId"] = json!(parent.str("id"));
  }
  let v = api.write("/1.0/comments/add", body).await?;
  let new_id = v.str("data.id").unwrap_or_default();
  Ok(Action::done("comment", id).with_id(new_id))
}

pub async fn delete(api: &Api, arg: &str, comment: &str) -> Result<Action> {
  let (_, kind) = posts::resolve(api, arg).await?;
  let body = json!({"id": comment.trim(), "targetType": kind.api_type()});
  api.write("/1.0/comments/remove", body).await?;
  Ok(Action::done("delete-comment", comment.trim()))
}
