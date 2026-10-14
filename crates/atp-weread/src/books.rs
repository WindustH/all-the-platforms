//! The store: search, rankings (榜单) and categories, one book, its public
//! reviews (点评).

use atp_core::{Comment, Error, Page, PageReq, Post, Query, Result, Value, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::refs;

/// `hot --category`: WeRead's rankings by a short name, with their ids.
const RANKINGS: &[(&str, &str)] = &[
  ("rising", "rising"),
  ("newbook", "newbook"),
  ("novel", "general_novel_rising"),
  ("all", "all"),
  ("newrating", "newrating"),
  ("potential", "potential"),
  ("hot-search", "hot_search"),
];
pub const RANKING_NAMES: &[&str] = &[
  "rising",
  "newbook",
  "novel",
  "all",
  "newrating",
  "potential",
  "hot-search",
];
/// The web's list type of a book's public reviews.
const PUBLIC_REVIEWS: &str = "3";
/// Entries per search page.
const SEARCH_PAGE: usize = 20;

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let offset = page.number_or(0);
  let params = [
    ("keyword", query.keyword.clone()),
    ("maxIdx", offset.to_string()),
    ("count", page.size_within(SEARCH_PAGE).to_string()),
    ("fragmentSize", "120".to_owned()),
  ];
  let v = api.get("/web/search/global", &params).await?;
  let rows = v.list("books");
  let items: Vec<Post> = rows
    .iter()
    .enumerate()
    .map(|(i, row)| parse::ranked(row, offset + i as u64 + 1))
    .collect();
  let last = rows
    .last()
    .and_then(|r| r.u64("searchIdx"))
    .unwrap_or(offset + rows.len() as u64);
  Ok(Page::new(
    items,
    more(&v, rows.len()).then(|| last.to_string()),
  ))
}

fn more(v: &Value, got: usize) -> bool {
  got > 0 && v.bool("hasMore").unwrap_or(false)
}

/// `hot`: a ranking (`rising` by default).
pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let name = category.unwrap_or("rising");
  let id = RANKINGS
    .iter()
    .find(|(n, _)| *n == name)
    .map(|(_, id)| *id)
    .ok_or_else(|| Error::input(format!("unknown ranking: {name}")))?;
  category_books(api, id, page).await
}

/// The books of a ranking or a store category (`100000` 精品小说 …), in
/// their order there.
pub async fn category_books(api: &Api, id: &str, page: &PageReq) -> Result<Page<Post>> {
  if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
    return Err(Error::input(format!(
      "not a WeRead category: {id} (give its id, such as 100000)"
    )));
  }
  let offset = page.number_or(0);
  let mut params = vec![("maxIndex", offset.to_string())];
  if !id.bytes().all(|b| b.is_ascii_digit()) {
    params.push(("rank", "1".into()));
  }
  let v = api
    .get(&format!("/web/bookListInCategory/{id}"), &params)
    .await?;
  let rows = v.list("books");
  let items: Vec<Post> = rows
    .iter()
    .enumerate()
    .map(|(i, row)| parse::ranked(row, offset + i as u64 + 1))
    .collect();
  let next = offset + rows.len() as u64;
  Ok(Page::new(
    items,
    more(&v, rows.len()).then(|| next.to_string()),
  ))
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = refs::book(arg)?;
  api.require_login()?;
  let v = api.get("/web/book/info", &[("bookId", id)]).await?;
  Ok(parse::book(&v))
}

/// Public reviews (点评) of a book, in the order WeRead lists them.
pub async fn reviews(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let id = refs::book(arg)?;
  api.require_login()?;
  let offset = page.number_or(0);
  let params = [
    ("bookId", id),
    ("listType", PUBLIC_REVIEWS.to_owned()),
    ("maxIdx", offset.to_string()),
    ("count", page.size_within(SEARCH_PAGE).to_string()),
    ("listMode", "2".to_owned()),
    ("synckey", "0".to_owned()),
  ];
  let v = api.get("/web/review/list", &params).await?;
  let rows = v.list("reviews");
  let items: Vec<Comment> = rows.iter().map(parse::review).collect();
  let next = rows
    .last()
    .and_then(|r| r.u64("idx"))
    .unwrap_or(offset + rows.len() as u64);
  let more = !rows.is_empty()
    && v
      .first_str(&["reviewsHasMore", "hasMore"])
      .is_some_and(|m| m == "1" || m == "true");
  Ok(Page::new(items, more.then(|| next.to_string())))
}
