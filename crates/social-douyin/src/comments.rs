//! Comments of a post and replies under one comment.

use social_core::{Comment, Page, PageReq, Result, Value, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

const LIST: &str = "/aweme/v1/web/comment/list/";
const REPLIES: &str = "/aweme/v1/web/comment/list/reply/";

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
