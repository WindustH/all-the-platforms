//! Community notes (笔记): a note (`16189/json/articleDetail`), its
//! comments (`20725/json/ruleSortCommentList`, cursor `nextPageKey`), and
//! the archived travel notes (游记), read from their pages. Ctrip stopped
//! the travel notes on 2025-11-30; they stay readable.

use atp_core::{Comment, Error, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::{Api, Site};
use crate::parse;
use crate::refs;

/// Parts of a note `articleDetail` answers.
const PARTS: &[&str] = &["CONTENT", "AUTHOR", "STATS", "POI", "TOPIC", "ARTICLE_TYPE"];
/// Comments a page holds.
const STEP: u64 = 10;

pub async fn read(api: &Api, id: &str) -> Result<Post> {
  let body = json!({
    "action": "get",
    "articleId": refs::number(id)?,
    "needParts": PARTS,
    "sharer": { "clientAuth": "", "token": "" },
    "flag": ""
  });
  let v = api
    .soa("16189/json/articleDetail", Site::Mobile, body)
    .await?;
  parse::note(v.at("article")).ok_or_else(|| Error::not_found(format!("no Ctrip note {id}")))
}

pub async fn comments(api: &Api, id: &str, page: &PageReq) -> Result<Page<Comment>> {
  let body = json!({
    "articleId": refs::number(id)?,
    "pagePara": {
      "nextPageKey": page.cursor.clone().unwrap_or_default(),
      "pageSize": STEP,
      "sortType": 0
    },
    "displayType": 2,
    "excludeCommentIds": [],
    "isContentEncode": false,
    "showCommentCount": true,
    "needHotelComment": false,
    "targetType": 1
  });
  let v = api
    .soa("20725/json/ruleSortCommentList", Site::Mobile, body)
    .await?;
  let items: Vec<Comment> = v
    .list("comments")
    .iter()
    .filter_map(parse::note_comment)
    .collect();
  let next = v
    .str("nextPageKey")
    .filter(|_| v.bool("hasMore") == Some(true) && !items.is_empty());
  Ok(Page::new(items, next))
}

/// A travel note (游记) from its page.
pub async fn travel(api: &Api, url: &str) -> Result<Post> {
  let html = api.page(url, Site::You).await?;
  parse::rsc_object(&html, "{\"data\":{\"Id\"")
    .and_then(|v| parse::travel(v.at("data"), url))
    .ok_or_else(|| Error::not_found(format!("no travel note in {url}")))
}
