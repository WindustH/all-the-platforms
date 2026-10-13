//! Comments (comment_v5): root comments of a post and replies under one comment.

use social_core::{Action, Comment, Page, PageReq, Result, Value, ValueExt, json};

use crate::api::{Api, V4};
use crate::refs::Target;
use crate::{api, parse};

/// `score` (hot first, the default) or `ts` (newest first).
pub async fn roots(
  api: &Api,
  target: &Target,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let url = format!(
    "{V4}/comment_v5/{}/{}/root_comment",
    target.plural(),
    target.id()
  );
  let req = api.get(&url).query("order_by", sort.unwrap_or("score"));
  list(api, req, page).await
}

pub async fn children(api: &Api, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
  let url = format!("{V4}/comment_v5/comment/{comment}/child_comment");
  list(api, api.get(&url).query("order_by", "ts"), page).await
}

/// The cursor is the upstream `offset` token of `paging.next`.
async fn list(api: &Api, req: social_core::http::Req<'_>, page: &PageReq) -> Result<Page<Comment>> {
  let req = req
    .query("limit", page.size_within(20))
    .query("offset", page.cursor.as_deref().unwrap_or(""));
  let v: Value = api.call(req).await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::comment).collect(),
    api::next_param(&v, "offset"),
  ))
}

pub async fn comment(
  api: &Api,
  target: &Target,
  text: &str,
  reply_to: Option<&str>,
) -> Result<Action> {
  api.prepare_write().await?;
  let mut body = json!({
    "content": text,
    "selected_settings": [],
    "unfriendly_check": "strict",
  });
  if let Some(parent) = reply_to {
    body["reply_comment_id"] = parent.into();
  }
  let url = format!(
    "{V4}/comment_v5/{}/{}/comment",
    target.plural(),
    target.id()
  );
  let v = api.call(api.post(&url).json(&body)).await?;
  let action = if reply_to.is_some() {
    "reply"
  } else {
    "comment"
  };
  let mut done = Action::done(action, target.url());
  if let Some(id) = v.str("id") {
    done = done.with_id(id);
  }
  Ok(done)
}

pub async fn delete_comment(api: &Api, target: &Target, comment: &str) -> Result<Action> {
  api.prepare_write().await?;
  api
    .call(api.delete(&format!("{V4}/comments/{comment}")))
    .await?;
  Ok(Action::done("delete_comment", target.url()).with_id(comment))
}

/// Like a comment (赞), or take the like back.
pub async fn like_comment(api: &Api, comment: &str, undo: bool) -> Result<Action> {
  api.prepare_write().await?;
  let url = format!("{V4}/comments/{comment}/like");
  let req = if undo {
    api.delete(&url)
  } else {
    api.post(&url)
  };
  api.call(req).await?;
  let action = if undo {
    "unlike_comment"
  } else {
    "like_comment"
  };
  Ok(Action::done(action, comment))
}
