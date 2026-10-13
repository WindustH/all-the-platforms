//! Pinned messages of a chat.

use atp_core::{Action, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::{Api, next_page, page_query};
use crate::{messages, refs};

pub async fn pin(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::message(arg)?;
  if undo {
    api.delete(&format!("/im/v1/pins/{id}")).await?;
    Ok(Action::done("unpin", &id))
  } else {
    api
      .post("/im/v1/pins", &[], json!({ "message_id": id }))
      .await?;
    Ok(Action::done("pin", &id))
  }
}

pub async fn pins(api: &Api, chat: &str, req: &PageReq) -> Result<Page<Post>> {
  let id = refs::chat(chat)?;
  let query = page_query(vec![("chat_id", id)], req, 50);
  let v = api.get("/im/v1/pins", &query).await?;
  let mut posts = Vec::new();
  for pin in v.list("data.items") {
    let Some(message) = pin.str("message_id") else {
      continue;
    };
    let mut post = messages::read(api, &message).await?;
    if let Some(by) = pin.str("operator_id") {
      post.extra.insert("pinned_by".into(), by.into());
    }
    posts.push(post);
  }
  Ok(Page::new(posts, next_page(&v)))
}
