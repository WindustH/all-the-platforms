//! Deals through the 美团联盟 API's `query_coupon`: keyword searches
//! (`searchText`, over every category, paged with the answer's
//! `searchId`), rankings (`listTopiId`, 20 deals, no further pages), plain
//! lists (`pageNo`, `hasNext`) and deals by id (`productViewSignList`, then
//! the older `vpSkuViewIds`). Without a location Meituan answers for
//! Beijing.

use atp_core::{Error, Page, PageReq, Post, Query, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;

/// `search --sort`: relevance (综合) or price, low to high.
pub const SORTS: &[&str] = &["relevance", "price"];
/// `hot --category`: the 到店 rankings 今日必推, 同城热销 and 实时热销.
pub const HOT: &[&str] = &["today", "local", "realtime"];
const PAGE_SIZE: usize = 20;

/// A ranking (`listTopiId`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Rank {
  /// 精选 (到家)
  Picks,
  /// 今日必推
  Today,
  /// 同城热销
  Local,
  /// 跟推爆款 (到家)
  Trending,
  /// 实时热销 (到店)
  Realtime,
}

impl Rank {
  fn id(self) -> u8 {
    match self {
      Rank::Picks => 1,
      Rank::Today => 2,
      Rank::Local => 3,
      Rank::Trending => 4,
      Rank::Realtime => 5,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Sort {
  Price,
  Sales,
  Commission,
}

/// What to list.
#[derive(Debug, Clone, Default)]
pub struct Filter {
  pub keyword: Option<String>,
  pub rank: Option<Rank>,
  /// 到店 instead of 到家.
  pub store: bool,
  pub city: Option<String>,
  pub area: Option<String>,
  /// Latitude and longitude.
  pub near: Option<(f64, f64)>,
  pub sort: Option<Sort>,
}

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let filter = Filter {
    keyword: Some(query.keyword.clone()),
    sort: (query.sort.as_deref() == Some("price")).then_some(Sort::Price),
    ..Filter::default()
  };
  list(api, &filter, page).await
}

pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let rank = match category.unwrap_or("today") {
    "local" => Rank::Local,
    "realtime" => Rank::Realtime,
    _ => Rank::Today,
  };
  let filter = Filter {
    rank: Some(rank),
    store: true,
    ..Filter::default()
  };
  list(api, &filter, page).await
}

/// One page of deals. Cursors are the page number, with `:<searchId>` for
/// searches.
pub async fn list(api: &Api, filter: &Filter, page: &PageReq) -> Result<Page<Post>> {
  let (number, search_id) = match page.cursor.as_deref().map(|c| c.split_once(':')) {
    Some(Some((n, id))) => (n.parse().unwrap_or(1), Some(id.to_owned())),
    _ => (page.number_or(1), None),
  };
  let size = page.size_within(PAGE_SIZE);
  let body = body(filter, number, search_id.as_deref(), size);
  let v = api.cps.call(&api.ctx, "query_coupon", &body).await?;
  let entries = v.list("data");
  let items: Vec<Post> = entries.iter().filter_map(parse::deal).collect();
  let more = v.bool("hasNext").unwrap_or(entries.len() >= size);
  let next = match (filter.rank, more && !entries.is_empty()) {
    (None, true) => Some(
      match v.str("searchId").filter(|_| filter.keyword.is_some()) {
        Some(id) => format!("{}:{id}", number + 1),
        None => (number + 1).to_string(),
      },
    ),
    _ => None,
  };
  Ok(Page::new(items, next))
}

fn body(filter: &Filter, number: u64, search_id: Option<&str>, size: usize) -> Value {
  let mut body = json!({});
  match (filter.rank, &filter.keyword) {
    (Some(rank), _) => body["listTopiId"] = json!(rank.id()),
    (None, Some(keyword)) => {
      body["searchText"] = json!(keyword);
      if let Some(id) = search_id {
        body["searchId"] = json!(id);
      }
    }
    (None, None) => {}
  }
  // Searches cover every category; Meituan refuses a platform with them.
  if filter.rank.is_some() || filter.keyword.is_none() {
    body["platform"] = json!(if filter.store { 2 } else { 1 });
  }
  if filter.rank.is_none() {
    body["pageNo"] = json!(number);
    body["pageSize"] = json!(size);
  }
  if let Some(city) = &filter.city {
    body["cityId"] = json!(city);
  }
  if let Some(area) = &filter.area {
    body["businessAreaId"] = json!(area);
  }
  if let Some((lat, lng)) = filter.near {
    body["latitude"] = json!((lat * 1e6).round() as i64);
    body["longitude"] = json!((lng * 1e6).round() as i64);
  }
  if let Some(field) = filter.sort.and_then(|s| sort_field(filter, s)) {
    body["sortField"] = json!(field);
  }
  body
}

/// `sortField`, whose values depend on what is listed.
fn sort_field(filter: &Filter, sort: Sort) -> Option<u8> {
  match (filter.rank.is_some(), filter.keyword.is_some(), sort) {
    (false, true, Sort::Price) => Some(2),
    (false, true, _) => None,
    (true, _, Sort::Price) if filter.store => None,
    (false, false, Sort::Commission) => None,
    (_, _, Sort::Price) => Some(1),
    (_, _, Sort::Sales) => Some(2),
    (_, _, Sort::Commission) => Some(3),
  }
}

/// A deal by its id (`productViewSign`, or an older `skuViewId`).
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = arg.trim();
  if id.is_empty()
    || !id
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
  {
    return Err(Error::input(format!(
      "not a 美团联盟 deal id: {arg} (give the id `search` or `hot` shows)"
    )));
  }
  for body in [
    json!({ "productViewSignList": [id] }),
    json!({ "vpSkuViewIds": id }),
  ] {
    let v = api.cps.call(&api.ctx, "query_coupon", &body).await?;
    if let Some(post) = v.list("data").iter().find_map(parse::deal) {
      return Ok(post);
    }
  }
  Err(Error::not_found(format!(
    "美团联盟 has no deal {id} on sale"
  )))
}
