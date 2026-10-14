//! Categories: the 多多进宝 goods tags (`pdd.goods.opt.get`, a tree) with
//! credentials, else the home page's tabs; and the goods of one, by a DDK
//! search with `opt_id` or the home tab's list
//! (`caterham/query/fenlei_gyl_group`, with `anti_content`).

use atp_core::{Collection, Error, Page, PageReq, Post, Result, ValueExt};

use crate::api::{Api, WEB, list_id};
use crate::parse;

const PAGE_SIZE: u64 = 20;

pub async fn list(api: &Api, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  let items = if api.ddk.has(&api.ctx) {
    let parent = parent.unwrap_or("0").to_owned();
    // openpdd sends `parent_cat_id`; PDD's documentation names `parent_opt_id`.
    let v = api
      .ddk
      .call(
        &api.ctx,
        "pdd.goods.opt.get",
        &[("parent_opt_id", parent.clone()), ("parent_cat_id", parent)],
      )
      .await?;
    v.list("goods_opt_get_response.goods_opt_list")
      .iter()
      .filter_map(parse::category)
      .collect()
  } else {
    if parent.is_some_and(|p| p != "0") {
      return Err(Error::input(
        "the home page has one level of tabs; sub-categories need 多多进宝 credentials",
      ));
    }
    let store = api.store(&format!("{WEB}/")).await?;
    store
      .list("operations")
      .iter()
      .filter_map(parse::tab)
      .collect()
  };
  Ok(atp_core::paging::slice(items, page))
}

/// Goods of a category; cursors are `<offset>:<list_id>` (home tabs) or
/// `<page>:<list_id>` (DDK).
pub async fn goods(api: &Api, opt: &str, page: &PageReq) -> Result<Page<Post>> {
  let (number, list) = page
    .cursor
    .as_deref()
    .and_then(|c| c.split_once(':'))
    .map(|(n, l)| (n.parse::<u64>().unwrap_or(0), l.to_owned()))
    .unwrap_or((0, String::new()));
  if api.ddk.has(&api.ctx) {
    return official(api, opt, number.max(1), list).await;
  }
  api.visit().await?;
  let list = if list.is_empty() { list_id() } else { list };
  let v = api
    .get(
      "/api/caterham/query/fenlei_gyl_group",
      &[
        ("opt_id", opt.to_owned()),
        ("offset", number.to_string()),
        ("list_id", list.clone()),
        ("opt_type", "1".into()),
        ("count", PAGE_SIZE.to_string()),
        ("opt_name", String::new()),
        ("platform", "H5".into()),
        ("page_id", "index.html?dy_sub_page=category".into()),
        ("page_sn", "10002".into()),
      ],
      &format!("{WEB}/"),
    )
    .await?;
  let items = parse::goods_list(&v);
  let next = (!items.is_empty()).then(|| format!("{}:{list}", number + items.len() as u64));
  Ok(Page::new(items, next))
}

async fn official(api: &Api, opt: &str, number: u64, list: String) -> Result<Page<Post>> {
  let pid = api.ddk.creds(&api.ctx).map(|c| c.pid).unwrap_or_default();
  let v = api
    .ddk
    .call(
      &api.ctx,
      "pdd.ddk.goods.search",
      &[
        ("opt_id", opt.to_owned()),
        ("page", number.to_string()),
        ("page_size", PAGE_SIZE.to_string()),
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
  let next = (items.len() as u64 >= PAGE_SIZE).then(|| format!("{}:{list}", number + 1));
  Ok(Page::new(items, next))
}
