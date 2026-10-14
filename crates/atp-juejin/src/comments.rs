//! Comments on articles and pins, the replies under them, and writing both.

use atp_core::{Action, Comment, Error, Page, PageReq, Result, ValueExt, json};

use crate::api::{AID, Api};
use crate::parse;
use crate::posts::next;
use crate::refs;

const PAGE_MAX: usize = 20;

pub async fn list(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let item = refs::item(arg)?;
  let body = json!({
    "client_type": AID,
    "cursor": page.cursor.clone().unwrap_or_else(|| "0".into()),
    "item_id": item.id(),
    "item_type": item.item_type(),
    "limit": page.size_within(PAGE_MAX),
  });
  let v = api.post("/interact_api/v1/comment/list", &body).await?;
  let items = v.list("data").iter().map(parse::comment).collect();
  Ok(Page::new(items, next(&v)))
}

pub async fn replies(api: &Api, arg: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
  let item = refs::item(arg)?;
  let body = json!({
    "client_type": AID,
    "cursor": page.cursor.clone().unwrap_or_else(|| "0".into()),
    "item_id": item.id(),
    "item_type": item.item_type(),
    "comment_id": comment,
    "limit": page.size_within(PAGE_MAX),
  });
  let v = api.post("/interact_api/v1/reply/list", &body).await?;
  let items = v.list("data").iter().map(parse::reply).collect();
  Ok(Page::new(items, next(&v)))
}

/// A comment on an article or pin, or a reply to one of its comments.
pub async fn publish(api: &Api, arg: &str, text: &str, reply_to: Option<&str>) -> Result<Action> {
  let item = refs::item(arg)?;
  if text.trim().is_empty() {
    return Err(Error::input("the comment is empty"));
  }
  let (path, body) = match reply_to {
    None => (
      "/interact_api/v1/comment/publish",
      json!({
        "client_type": AID,
        "item_id": item.id(),
        "item_type": item.item_type(),
        "comment_content": text,
        "comment_pics": [],
      }),
    ),
    Some(comment) => (
      "/interact_api/v1/reply/publish",
      json!({
        "client_type": AID,
        "item_id": item.id(),
        "item_type": item.item_type(),
        "reply_to_comment_id": comment,
        "reply_to_reply_id": "0",
        "reply_to_user_id": "0",
        "reply_content": text,
        "reply_pics": [],
      }),
    ),
  };
  let v = api.write(path, &body).await?;
  let id = v
    .first_str(&[
      "data.comment_id",
      "data.reply_id",
      "data.comment_info.comment_id",
    ])
    .unwrap_or_default();
  Ok(Action::done("comment", arg).with_id(id))
}
