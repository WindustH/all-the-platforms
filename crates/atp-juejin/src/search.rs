//! Search: articles and users, as the site's search page asks.

use atp_core::{Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::posts::next;

/// `search --sort`: relevance, newest or hottest first.
pub const SORTS: &[&str] = &["relevance", "newest", "hottest"];

/// One page of results of `id_type` (2 articles, 1 users).
async fn results(api: &Api, query: &Query, id_type: u64, page: &PageReq) -> Result<Value> {
  let sort = match query.sort.as_deref().unwrap_or("relevance") {
    "relevance" => 0,
    "newest" => 1,
    "hottest" => 2,
    other => return Err(Error::input(format!("unknown order: {other}"))),
  };
  let params = [
    ("query", query.keyword.clone()),
    ("id_type", id_type.to_string()),
    ("cursor", page.cursor.clone().unwrap_or_else(|| "0".into())),
    ("limit", page.size_within(20).to_string()),
    ("search_type", "0".to_owned()),
    ("sort_type", sort.to_string()),
    ("version", "1".to_owned()),
  ];
  api.get("/search_api/v1/search", &params).await
}

pub async fn articles(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let v = results(api, query, 2, page).await?;
  let items = v
    .list("data")
    .iter()
    .filter(|row| row.u64("result_type") == Some(2))
    .map(|row| parse::article(row.at("result_model")))
    .collect();
  Ok(Page::new(items, next(&v)))
}

pub async fn users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let v = results(api, query, 1, page).await?;
  let items = v
    .list("data")
    .iter()
    .filter(|row| row.u64("result_type") == Some(1))
    .map(|row| {
      let model = row.at("result_model");
      let found = model.at("user_info");
      parse::user(if found.is_object() { found } else { model })
    })
    .collect();
  Ok(Page::new(items, next(&v)))
}
