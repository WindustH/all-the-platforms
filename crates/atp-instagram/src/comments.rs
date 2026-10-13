//! Comments of a post and their replies, commenting and deleting comments.

use atp_core::{Action, Comment, Error, Page, PageReq, Reply, Result, ValueExt};

use crate::api::{Api, with_cursor};
use crate::parse;
use crate::refs;

/// Comments, newest batch first; logged out only the ones the post page ships.
pub async fn comments(api: &Api, post: &str, req: &PageReq) -> Result<Page<Comment>> {
  if !api.has_session() {
    let media = api.public_post(post).await?;
    let list = media
      .list("edge_media_to_parent_comment.edges")
      .iter()
      .map(|edge| parse::comment(edge.at("node")))
      .collect();
    return Ok(Page::new(list, None));
  }
  let id = refs::media_id(post)?;
  let query = with_cursor(
    vec![
      ("can_support_threading", "true".to_owned()),
      ("permalink_enabled", "false".to_owned()),
    ],
    "min_id",
    req.cursor.as_ref(),
  );
  let v = api.get(&format!("media/{id}/comments/"), &query).await?;
  let list = v.list("comments").iter().map(parse::comment).collect();
  let next = v.str("next_min_id").filter(|_| {
    v.bool("has_more_headload_comments") != Some(false) || v.str("next_min_id").is_some()
  });
  Ok(Page::new(list, next))
}

pub async fn replies(api: &Api, post: &str, comment: &str, req: &PageReq) -> Result<Page<Comment>> {
  api.require_session("comment replies")?;
  let id = refs::media_id(post)?;
  let query = with_cursor(Vec::new(), "max_id", req.cursor.as_ref());
  let v = api
    .get(
      &format!("media/{id}/comments/{comment}/child_comments/"),
      &query,
    )
    .await?;
  let list = v
    .list("child_comments")
    .iter()
    .map(parse::comment)
    .collect();
  let next = (v.bool("has_more_tail_child_comments") == Some(true))
    .then(|| v.str("next_max_child_cursor"))
    .flatten();
  Ok(Page::new(list, next))
}

/// Comment on a post, or answer one of its comments (`--reply-to`).
pub async fn comment(api: &Api, post: &str, reply: &Reply) -> Result<Action> {
  api.require_session("commenting")?;
  if !reply.images.is_empty() {
    return Err(Error::unsupported("comment --image"));
  }
  let id = refs::media_id(post)?;
  let mut form = vec![("comment_text", reply.text.clone())];
  if let Some(comment) = &reply.reply_to {
    form.push(("replied_to_comment_id", comment.clone()));
  }
  let v = api.post(&format!("/web/comments/{id}/add/"), &form).await?;
  let verb = if reply.reply_to.is_some() {
    "reply"
  } else {
    "comment"
  };
  Ok(Action::done(verb, &id).with_id(v.str("id").unwrap_or_default()))
}

pub async fn delete(api: &Api, post: &str, comment: &str) -> Result<Action> {
  api.require_session("deleting comments")?;
  let id = refs::media_id(post)?;
  api
    .post(&format!("/web/comments/{id}/delete/{comment}/"), &[])
    .await?;
  Ok(Action::done("delete-comment", comment))
}
