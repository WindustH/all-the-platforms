//! Comments: of a facebook.com post (the page's first batch, then its
//! comment operation replayed) or of a Page post through the Graph API, and
//! commenting on and deleting from Page posts.

use atp_core::http::Part;
use atp_core::{Action, Comment, Error, ErrorCode, Page, PageReq, Reply, Result, ValueExt};
use atp_meta::graph::params;

use crate::api::{Api, photo_part};
use crate::parse;
use crate::refs::{self, graph_id, is_graph_id};
use crate::web::{advance, stop};

const COMMENT_FIELDS: &str = "id,message,created_time,from{id,name},like_count,comment_count";

pub async fn comments(api: &Api, post: &str, req: &PageReq) -> Result<Page<Comment>> {
  if is_graph_id(post) {
    return graph_comments(api, post, req).await;
  }
  let page = api.page(&refs::post_url(post)?).await?;
  page.check("post", &[])?;
  let story =
    parse::permalink(&page.docs).ok_or_else(|| Error::not_found("the page carried no post"))?;
  let author = story.author.as_ref().map(|a| a.id.clone());
  let raw = story.raw.clone().unwrap_or_default();
  let Some(cursor) = &req.cursor else {
    let (list, next) = parse::comments(&raw, author.as_deref());
    return Ok(Page::new(list, next));
  };
  let Some(operation) = page
    .preloads
    .iter()
    .find(|p| p.has_cursor() && (p.op.contains("Comment") || p.op.contains("UFI")))
  else {
    return Ok(stop(api, "these comments"));
  };
  let data = match advance(&api.ctx, &page, operation, cursor, req.size_within(50)).await {
    Err(e) if e.code == ErrorCode::NotAuthenticated => return Ok(stop(api, "these comments")),
    other => other?,
  };
  let (list, next) = parse::comments(&data, author.as_deref());
  Ok(Page::new(list, next))
}

/// Top-level comments of a Page post, newest first.
async fn graph_comments(api: &Api, post: &str, req: &PageReq) -> Result<Page<Comment>> {
  let id = graph_id(post)?;
  let fields = format!("{COMMENT_FIELDS},parent{{id}}");
  let query = params(&[
    ("fields", &fields),
    ("order", "reverse_chronological"),
    ("filter", "toplevel"),
  ]);
  let graph = api.graph_for(&id).await?;
  let (items, next) = graph
    .page(&format!("{id}/comments"), &query, req, 100)
    .await?;
  Ok(Page::new(
    items.iter().map(parse::graph::comment).collect(),
    next,
  ))
}

/// Replies of a Page post's comment (Graph API only).
pub async fn replies(api: &Api, comment: &str, req: &PageReq) -> Result<Page<Comment>> {
  if !is_graph_id(comment) {
    return Err(
      Error::unsupported("replies of a facebook.com comment").with_hint(
        "replies are listed for Page posts read through the Graph API (comment ids like 123_456)",
      ),
    );
  }
  let id = graph_id(comment)?;
  let graph = api.graph_for(&id).await?;
  let (items, next) = graph
    .page(
      &format!("{id}/comments"),
      &params(&[("fields", COMMENT_FIELDS)]),
      req,
      100,
    )
    .await?;
  Ok(Page::new(
    items.iter().map(parse::graph::comment).collect(),
    next,
  ))
}

/// Comment on a Page post, or answer one of its comments (`--reply-to`).
pub async fn comment(api: &Api, post: &str, reply: &Reply) -> Result<Action> {
  let post = graph_id(post)?;
  let target = match &reply.reply_to {
    Some(comment) => graph_id(comment)?,
    None => post.clone(),
  };
  let graph = api.graph_for(&post).await?;
  let path = format!("{target}/comments");
  let v = match reply.images.first() {
    None => {
      let form = vec![("message".to_owned(), reply.text.clone())];
      graph.post(&path, &form).await?
    }
    Some(image) => {
      let parts = vec![
        photo_part(image).await?,
        Part::text("message", reply.text.clone()),
      ];
      graph.post_multipart(&path, parts).await?
    }
  };
  Ok(Action::done("comment", &target).with_id(v.str("id").unwrap_or_default()))
}

pub async fn delete(api: &Api, post: &str, comment: &str) -> Result<Action> {
  let post = graph_id(post)?;
  let comment = graph_id(comment)?;
  api.graph_for(&post).await?.delete(&comment).await?;
  Ok(Action::done("delete-comment", &comment))
}
