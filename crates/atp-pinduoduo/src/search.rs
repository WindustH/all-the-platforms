//! `search`: the 多多进宝 goods search (`pdd.ddk.goods.search`) when the
//! session has DDK credentials, else the H5 search (`POST /proxy/api/search`
//! with `anti_content`), which needs a login and is throttled hard.

use atp_core::{Error, Page, PageReq, Post, Query, Result, ValueExt, json};

use crate::api::{Api, list_id, search_url};
use crate::parse;

pub const SORTS: &[&str] = &["relevance", "sales", "price-asc", "price-desc"];
const PAGE_SIZE: usize = 20;
/// DDK pages: one fixed size, so page numbers keep their offsets.
const DDK_PAGE: usize = 50;

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  if api.ddk.has(&api.ctx) {
    return official(api, query, page).await;
  }
  if api.has_session() {
    return web(api, query, page).await;
  }
  Err(
    Error::auth("searching Pinduoduo needs 多多进宝 credentials or a login")
      .with_hint(api.either_hint()),
  )
}

/// DDK search; cursors are `<page>:<list_id>`.
async fn official(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let (number, list) = cursor(page);
  let size = DDK_PAGE;
  let sort = match query.sort.as_deref() {
    Some("sales") => 6,
    Some("price-asc") => 9,
    Some("price-desc") => 10,
    _ => 0,
  };
  let pid = api.ddk.creds(&api.ctx).map(|c| c.pid).unwrap_or_default();
  let v = api
    .ddk
    .call(
      &api.ctx,
      "pdd.ddk.goods.search",
      &[
        ("keyword", query.keyword.clone()),
        ("page", number.to_string()),
        ("page_size", size.to_string()),
        ("sort_type", sort.to_string()),
        ("list_id", list),
        ("pid", pid),
      ],
    )
    .await?;
  let r = v.at("goods_search_response");
  let items: Vec<Post> = r
    .list("goods_list")
    .iter()
    .filter_map(parse::ddk_goods)
    .collect();
  let list = r.str("list_id").unwrap_or_default();
  let total = r.u64("total_count").unwrap_or(u64::MAX);
  let more = items.len() >= size && (number * size as u64) < total;
  Ok(Page::new(
    items,
    more.then(|| format!("{}:{list}", number + 1)),
  ))
}

/// `<page>:<rest>` with page 1 and an empty rest first.
fn cursor(page: &PageReq) -> (u64, String) {
  page
    .cursor
    .as_deref()
    .and_then(|c| c.split_once(':'))
    .map(|(n, rest)| (n.parse().unwrap_or(1), rest.to_owned()))
    .unwrap_or((1, String::new()))
}

/// The H5 search, 20 a page; cursors are `<page>:<list_id>:<flip>`.
async fn web(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let (number, rest) = cursor(page);
  let (list, flip) = rest
    .split_once(':')
    .map(|(l, f)| (l.to_owned(), f.to_owned()))
    .unwrap_or_else(|| (list_id(), String::new()));
  let sort = match query.sort.as_deref() {
    Some("sales") => "_sales",
    Some("price-asc") => "_price",
    Some("price-desc") => "price",
    _ => "default",
  };
  let mut body = json!({
    "item_ver": "lzqq", "support_enhance_type": 1, "coupon_price_flag": 1,
    "source": "search", "search_met": "manual", "list_id": list, "sort": sort,
    "filter": "", "q": query.keyword, "page": number,
    "is_new_query": u8::from(number == 1), "size": PAGE_SIZE,
  });
  if !flip.is_empty() {
    body["flip"] = json!(flip);
  }
  let from = format!("{}&search_met=manual", search_url(&query.keyword));
  let v = api.post("/search", body, &from).await?;
  let items = parse::goods_list(&v);
  let flip = v.str("flip").unwrap_or_default();
  let more = !items.is_empty() && v.bool("has_more") != Some(false);
  Ok(Page::new(
    items,
    more.then(|| format!("{}:{list}:{flip}", number + 1)),
  ))
}

/// The `goods_sign` of a goods id, from a DDK search for the id (DDK
/// takes goods ids and links as keywords).
pub async fn sign_of(api: &Api, goods_id: &str) -> Result<String> {
  let pid = api.ddk.creds(&api.ctx).map(|c| c.pid).unwrap_or_default();
  let v = api
    .ddk
    .call(
      &api.ctx,
      "pdd.ddk.goods.search",
      &[("keyword", goods_id.to_owned()), ("pid", pid)],
    )
    .await?;
  v.str("goods_search_response.goods_list.0.goods_sign")
    .ok_or_else(|| Error::not_found(format!("多多进宝 does not list goods {goods_id}")))
}
