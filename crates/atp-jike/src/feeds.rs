//! Feeds and search: the following feed (`/1.0/personalUpdate/
//! followingUpdates`), the recommended feed of the web app's 发现 page
//! (`/1.0/recommendFeed/list`), search (`/1.0/search/integrate`, which
//! answers posts, users and topics in one list) and your saved posts
//! (`/1.0/collections/list`).

use atp_core::{Collection, Page, PageReq, Post, Query, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;

async fn posts(api: &Api, path: &str, body: Value, req: &PageReq) -> Result<Page<Post>> {
  let (v, next) = api.list(path, body, req).await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::post).collect(),
    next,
  ))
}

/// The posts and updates of the people you follow.
pub async fn following(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  posts(api, "/1.0/personalUpdate/followingUpdates", json!({}), req).await
}

/// The recommended feed (发现).
pub async fn recommended(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  posts(api, "/1.0/recommendFeed/list", json!({}), req).await
}

/// Your saved posts (收藏).
pub async fn saved(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  posts(api, "/1.0/collections/list", json!({}), req).await
}

/// One page of search results, all kinds together.
async fn search_page(
  api: &Api,
  words: &str,
  req: &PageReq,
) -> Result<(Vec<Value>, Option<String>)> {
  let (v, next) = api
    .list("/1.0/search/integrate", json!({"keywords": words}), req)
    .await?;
  Ok((v.list("data").to_vec(), next))
}

fn of_type<'a>(items: &'a [Value], kind: &'a str) -> impl Iterator<Item = &'a Value> {
  items
    .iter()
    .filter(move |item| item.str("type").as_deref() == Some(kind))
}

pub async fn search(api: &Api, query: &Query, req: &PageReq) -> Result<Page<Post>> {
  let (items, next) = search_page(api, &query.keyword, req).await?;
  let found = of_type(&items, "ORIGINAL_POST").map(parse::post).collect();
  Ok(Page::new(found, next))
}

pub async fn search_users(api: &Api, query: &Query, req: &PageReq) -> Result<Page<User>> {
  let (items, next) = search_page(api, &query.keyword, req).await?;
  let found = of_type(&items, "USER_SECTION")
    .flat_map(|section| section.list("items"))
    .map(parse::user)
    .collect();
  Ok(Page::new(found, next))
}

pub async fn search_topics(api: &Api, query: &Query, req: &PageReq) -> Result<Page<Collection>> {
  let (items, next) = search_page(api, &query.keyword, req).await?;
  let found = of_type(&items, "TOPIC").map(parse::topic).collect();
  Ok(Page::new(found, next))
}
