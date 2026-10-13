//! Feeds besides recommendations: the follow feed (关注), a user's
//! activities (动态) and the account's read history (最近浏览).

use social_core::{Ctx, Page, PageReq, Post, Result, ValueExt};

use crate::api::{self, V3};
use crate::parse;

/// Activity of the people and topics you follow; the cursor is the query of `paging.next`.
pub async fn follow(ctx: &Ctx, page: &PageReq) -> Result<Page<Post>> {
  moments(ctx, &format!("{V3}/moments"), page).await
}

/// What a user did lately: answered, posted, voted, followed ...
pub async fn activities(ctx: &Ctx, token: &str, page: &PageReq) -> Result<Page<Post>> {
  moments(ctx, &format!("{V3}/moments/{token}/activities"), page).await
}

async fn moments(ctx: &Ctx, base: &str, page: &PageReq) -> Result<Page<Post>> {
  let req = match &page.cursor {
    Some(query) => api::get(ctx, &format!("{base}?{query}")),
    None => api::get(ctx, base).queries([
      ("limit", page.size_within(20).to_string()),
      ("desktop", "true".to_owned()),
    ]),
  };
  let v = api::call(ctx, req).await?;
  let items = v.list("data").iter().filter_map(parse::activity).collect();
  Ok(Page::new(items, api::next_query(&v)))
}

/// Content you read, newest first, with `extra.read_at`.
pub async fn history(ctx: &Ctx, page: &PageReq) -> Result<Page<Post>> {
  let req = api::mobile(ctx, "/unify-consumption/read_history").queries([
    ("offset", page.number_or(0)),
    ("limit", page.size_within(20) as u64),
  ]);
  let v = api::call(ctx, req).await?;
  let items = v
    .list("data")
    .iter()
    .filter_map(parse::history_item)
    .collect();
  Ok(Page::new(items, api::next_param(&v, "offset")))
}
