//! The hot ranking (热榜) and the home feeds: recommended and newest
//! articles, recommended and hot pins.

use atp_core::paging::slice;
use atp_core::{Error, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::{AID, Api};
use crate::parse;
use crate::posts::next;

/// `hot --category`: the ranking of all articles or of one category.
const CATEGORIES: &[(&str, &str)] = &[
  ("all", "1"),
  ("backend", "6809637769959178254"),
  ("frontend", "6809637767543259144"),
  ("android", "6809635626879549454"),
  ("ios", "6809635626661445640"),
  ("ai", "6809637773935378440"),
  ("tools", "6809637771511070734"),
  ("career", "6809637776263217160"),
  ("reading", "6809637772874219534"),
];
pub const CATEGORY_NAMES: &[&str] = &[
  "all", "backend", "frontend", "android", "ios", "ai", "tools", "career", "reading",
];
/// `feed --type`.
pub const FEEDS: &[&str] = &["recommend", "newest", "pins", "pins-hot"];
const PAGE_MAX: usize = 20;

/// The article ranking of a category (`all` by default), fifty entries.
pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let name = category.unwrap_or("all");
  let id = CATEGORIES
    .iter()
    .find(|(n, _)| *n == name)
    .map(|(_, id)| *id)
    .ok_or_else(|| Error::input(format!("unknown category: {name}")))?;
  let params = [("category_id", id.to_owned()), ("type", "hot".to_owned())];
  let v = api
    .get("/content_api/v1/content/article_rank", &params)
    .await?;
  let items = v
    .list("data")
    .iter()
    .enumerate()
    .map(|(i, row)| parse::ranked(row, i + 1))
    .collect();
  Ok(slice(items, page))
}

pub async fn feed(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let cursor = page.cursor.clone().unwrap_or_else(|| "0".into());
  let limit = page.size_within(PAGE_MAX);
  match kind.unwrap_or("recommend") {
    "recommend" => articles(api, 200, cursor, limit).await,
    "newest" => articles(api, 300, cursor, limit).await,
    "pins" => pins(api, "recommend", 300, cursor, limit).await,
    "pins-hot" => pins(api, "hot", 200, cursor, limit).await,
    other => Err(Error::input(format!("unknown feed: {other}"))),
  }
}

/// The home page's article feed (`sort_type` 200 recommended, 300 newest).
async fn articles(api: &Api, sort: u64, cursor: String, limit: usize) -> Result<Page<Post>> {
  let body = json!({
    "id_type": 2,
    "client_type": AID,
    "sort_type": sort,
    "cursor": cursor,
    "limit": limit,
  });
  let v = api
    .post("/recommend_api/v1/article/recommend_all_feed", &body)
    .await?;
  // Entries of other types (ads, courses) are left out.
  let items = v
    .list("data")
    .iter()
    .filter(|row| row.u64("item_type") == Some(2))
    .map(|row| parse::article(row.at("item_info")))
    .collect();
  Ok(Page::new(items, next(&v)))
}

/// The pins page's feeds (`recommend` or `hot`).
async fn pins(
  api: &Api,
  which: &str,
  sort: u64,
  cursor: String,
  limit: usize,
) -> Result<Page<Post>> {
  let body = json!({"id_type": 4, "sort_type": sort, "cursor": cursor, "limit": limit});
  let v = api
    .post(&format!("/recommend_api/v1/short_msg/{which}"), &body)
    .await?;
  let items = v.list("data").iter().map(parse::pin).collect();
  Ok(Page::new(items, next(&v)))
}
