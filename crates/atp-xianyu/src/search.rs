//! `search`: the web app's item search, 30 items a page, by relevance,
//! newest first or by price. When nothing matches, Xianyu answers related
//! items instead and says so.

use atp_core::output::note;
use atp_core::{Page, PageReq, Post, Query, Result, ValueExt, json};

use crate::api::{self, Api};
use crate::parse;

pub const SORTS: &[&str] = &["relevance", "newest", "price-asc", "price-desc"];
const PAGE: u64 = 30;
/// The web app pages no further.
const LAST_PAGE: u64 = 50;

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let number = page.number_or(1);
  let (field, order) = match query.sort.as_deref() {
    Some("newest") => ("create", "desc"),
    Some("price-asc") => ("price", "asc"),
    Some("price-desc") => ("price", "desc"),
    _ => ("", ""),
  };
  let data = json!({
    "pageNumber": number,
    "keyword": query.keyword,
    "fromFilter": !field.is_empty(),
    "rowsPerPage": PAGE,
    "sortValue": order,
    "sortField": field,
    "customDistance": "",
    "gps": "",
    "propValueStr": {"searchFilter": ""},
    "customGps": "",
    "searchReqFromPage": "pcSearch",
    "extraFilterValue": "{}",
    "userPositionJson": "{}",
  });
  let v = api
    .call("mtop.taobao.idlemtopsearch.pc.search", data, api::SEARCH)
    .await?;
  let fields = v.at("resultInfo.searchResControlFields");
  if fields.bool("similar") == Some(true) && number == 1 {
    note("no item matches exactly; Xianyu shows related items");
  }
  let items: Vec<Post> = v
    .list("resultList")
    .iter()
    .filter_map(parse::search_item)
    .collect();
  let total = fields.u64("numFound").unwrap_or(0);
  let more = !items.is_empty() && number < LAST_PAGE && (number * PAGE < total || total == 0);
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}
