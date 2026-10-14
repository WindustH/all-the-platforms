//! Comment threads, rebuilt from Lemmy's comment paths; writing and
//! deleting comments.
//!
//! A thread is fetched as Lemmy's own pages do: one request with a depth
//! limit, which Lemmy answers with up to 300 comments and no pages; the
//! top-level comments are then paged here.

use atp_core::paging::slice;
use atp_core::{Action, Comment, Error, Page, PageReq, Reply, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::posts;
use crate::refs;

pub const SORTS: &[&str] = &["hot", "top", "new", "old", "controversial"];
/// How deep one request follows a thread.
const DEPTH: usize = 8;

fn sort(name: Option<&str>) -> Result<String> {
  let name = name.unwrap_or("hot");
  if !SORTS.contains(&name) {
    return Err(Error::input(format!("unknown comment sort: {name}")));
  }
  let mut chars = name.chars();
  Ok(
    chars
      .next()
      .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
      .unwrap_or_default(),
  )
}

async fn thread(api: &Api, key: &str, id: u64, order: &str) -> Result<Vec<Value>> {
  let params = vec![
    (key, id.to_string()),
    ("max_depth", DEPTH.to_string()),
    ("sort", order.to_owned()),
    ("type_", "All".to_owned()),
  ];
  let v = api.get("comment/list", params).await?;
  Ok(v.list("comments").to_vec())
}

/// A post's comments, each with its replies nested.
pub async fn list(
  api: &Api,
  arg: &str,
  order: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let post = posts::post_id(api, arg).await?;
  let rows = thread(api, "post_id", post, &sort(order)?).await?;
  Ok(slice(parse::tree(&rows, 0, &api.instance()), page))
}

/// The replies under one comment, nested.
pub async fn replies(api: &Api, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
  let id = refs::comment_id(comment, &api.instance())?;
  let rows = thread(api, "parent_id", id, "Hot").await?;
  Ok(slice(parse::tree(&rows, id, &api.instance()), page))
}

pub async fn comment(api: &Api, post: &str, reply: &Reply) -> Result<Action> {
  api.require()?;
  if !reply.images.is_empty() {
    return Err(Error::unsupported("comment --image"));
  }
  let post_id = posts::post_id(api, post).await?;
  let mut body = json!({"content": reply.text, "post_id": post_id});
  if let Some(parent) = &reply.reply_to {
    body["parent_id"] = json!(refs::comment_id(parent, &api.instance())?);
  }
  let v = api.write("comment", body).await?;
  let id = v.str("comment_view.comment.id").unwrap_or_default();
  Ok(
    Action::done("comment", post)
      .with_id(id.clone())
      .with_url(format!("https://{}/comment/{id}", api.instance())),
  )
}

pub async fn delete(api: &Api, comment: &str) -> Result<Action> {
  let id = refs::comment_id(comment, &api.instance())?;
  api
    .write("comment/delete", json!({"comment_id": id, "deleted": true}))
    .await?;
  Ok(Action::done("delete-comment", comment))
}
