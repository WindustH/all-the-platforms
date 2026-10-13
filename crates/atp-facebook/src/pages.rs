//! Facebook Pages through the Graph API (a token from `login --token` or
//! FACEBOOK_ACCESS_TOKEN): the Pages a token manages, the one to act on by
//! default, and a Page's posts.
//!
//! The Page to act on: `--topic PAGE` / an id in the argument,
//! FACEBOOK_PAGE_ID, the one saved by `use-page`, or the only one managed.

use atp_core::{Action, Collection, Page, PageReq, Post, Result, ValueExt};
use atp_meta::graph::fields;

use crate::api::{Api, PAGE_KEY};
use crate::parse;
use crate::parse::graph::POST_FIELDS;

/// The Pages the token manages.
pub async fn pages(api: &Api) -> Result<Vec<Collection>> {
  Ok(api.pages().await?.iter().map(parse::graph::page).collect())
}

pub async fn use_page(api: &Api, page: &str) -> Result<Action> {
  let v = api.graph()?.get(page, &fields("id,name")).await?;
  let id = v.str("id").unwrap_or_else(|| page.to_owned());
  api.ctx.set_extra(PAGE_KEY, &id);
  Ok(
    Action::done("use-page", v.str("name").unwrap_or_else(|| id.clone()))
      .with_id(id)
      .with_message("used by default from now on"),
  )
}

pub async fn page_posts(api: &Api, page: Option<&str>, req: &PageReq) -> Result<Page<Post>> {
  let page = api.page_id(page).await?;
  let graph = api.page_graph(&page).await?;
  let (items, next) = graph
    .page(&format!("{page}/posts"), &fields(POST_FIELDS), req, 100)
    .await?;
  Ok(Page::new(
    items.iter().map(parse::graph::post).collect(),
    next,
  ))
}
