//! Comments of a post and replies under one comment; commenting, replying
//! and deleting one's comments.

use atp_core::{Action, Comment, Page, PageReq, Reply, Result, Value, ValueExt};

use crate::api::Api;
use crate::{parse, refs, users};

const LIST: &str = "/aweme/v1/web/comment/list/";
const REPLIES: &str = "/aweme/v1/web/comment/list/reply/";
const PUBLISH: &str = "/aweme/v1/web/comment/publish";
const DELETE: &str = "/aweme/v1/web/comment/delete/";

fn page_of(v: &Value, cursor: u64) -> Page<Comment> {
  let items: Vec<Comment> = v
    .list("comments")
    .iter()
    .filter_map(parse::comment)
    .collect();
  let next = (v.i64("has_more") == Some(1) && !items.is_empty()).then(|| {
    v.u64("cursor")
      .unwrap_or(cursor + items.len() as u64)
      .to_string()
  });
  Page::new(items, next)
}

pub async fn list(api: &Api, post: &str, page: &PageReq) -> Result<Page<Comment>> {
  let id = refs::aweme(api, post).await?;
  let cursor = page.number_or(0);
  let v = api
    .get(LIST)
    .arg("aweme_id", &id)
    .arg("cursor", cursor)
    .arg("count", page.size_within(20))
    .arg("item_type", "0")
    .arg("insert_ids", "")
    .arg("whale_cut_token", "")
    .arg("cut_version", "1")
    .arg("rcFT", "")
    .arg("version_code", "170400")
    .arg("version_name", "17.4.0")
    .send()
    .await?;
  Ok(page_of(&v, cursor))
}

pub async fn replies(
  api: &Api,
  post: &str,
  comment: &str,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let id = refs::aweme(api, post).await?;
  let cursor = page.number_or(0);
  let v = api
    .get(REPLIES)
    .arg("item_id", &id)
    .arg("comment_id", comment.trim())
    .arg("cut_version", "1")
    .arg("cursor", cursor)
    .arg("count", page.size_within(20))
    .arg("item_type", "0")
    .arg("version_code", "170400")
    .arg("version_name", "17.4.0")
    .send()
    .await?;
  Ok(page_of(&v, cursor))
}

/// Comment on a post, or reply to one of its comments (`reply.reply_to`).
pub async fn send(api: &Api, post: &str, reply: &Reply) -> Result<Action> {
  let id = refs::aweme(api, post).await?;
  let uid = users::uid_hash(api).await?;
  let mut call = api
    .write(PUBLISH, true, &uid)
    .await?
    .header("referer", refs::video_url(&id))
    .arg("app_name", "aweme")
    .arg("enter_from", "video_detail")
    .arg("previous_page", "video_detail")
    .field("aweme_id", &id);
  if let Some(comment) = &reply.reply_to {
    call = call.field("reply_id", comment.trim());
  }
  let v = call
    .field("comment_send_celltime", "0")
    .field("comment_video_celltime", "0")
    .field("one_level_comment_rank", "-1")
    .field("paste_edit_method", "non_paste")
    .field("text", &reply.text)
    .field("text_extra", "[]")
    .send()
    .await?;
  let action = Action::done("comment", &id).with_url(refs::video_url(&id));
  Ok(match v.str("comment.cid") {
    Some(cid) => action.with_id(cid),
    None => action,
  })
}

/// Delete one of the account's comments.
pub async fn delete(api: &Api, post: &str, comment: &str) -> Result<Action> {
  let id = refs::aweme(api, post).await?;
  let uid = users::uid_hash(api).await?;
  api
    .write(DELETE, true, &uid)
    .await?
    .header("referer", refs::video_url(&id))
    .field("cid", comment.trim())
    .field("aweme_id", &id)
    .send()
    .await?;
  Ok(Action::done("delete-comment", comment.trim()).with_url(refs::video_url(&id)))
}
