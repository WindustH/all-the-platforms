//! `search` (the PC search's `pc_search_searchWare`, h5st-signed, 30 items
//! a call), `hot` (`pc_search_hotwords`) and related words
//! (`pc_search_relwords`, `relsearch`), which pages send unsigned.

use atp_core::{Page, PageReq, Post, Query, Result, Value, ValueExt, json, paging};

use crate::api::{Api, Call, From, SEARCH_ORIGIN};
use crate::parse;

pub const SORTS: &[&str] = &["relevance", "sales", "price-asc", "price-desc", "comments"];
/// The search page's h5st app id.
const SIGN_APP: &str = "f06cc";
const APPID: &str = "search-pc-java";
const PAGE_SIZE: u64 = 30;
const ORDER: &[&str] = &[
  "appid",
  "t",
  "client",
  "clientVersion",
  "cthr",
  "uuid",
  "loginType",
  "keyword",
  "functionId",
  "body",
  "x-api-eid-token",
  "h5st",
];
const PLAIN_ORDER: &[&str] = &[
  "appid",
  "functionId",
  "client",
  "clientVersion",
  "uuid",
  "keyword",
  "num",
  "rettype",
  "type_name",
  "body",
  "t",
];

/// Cursors are `<call>.<offset>`: the page asks for the next call from
/// one past the last natural (non-ad) result.
pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let (number, start) = page
    .cursor
    .as_deref()
    .and_then(|c| c.split_once('.'))
    .and_then(|(n, s)| Some((n.parse::<u64>().ok()?, s.parse::<u64>().ok()?)))
    .unwrap_or((1, 1));
  let mut body = json!({
    "enc": "utf-8", "area": api.area(), "page": number, "mode": "", "concise": false,
    "hoverPictures": false, "newAdvRepeat": false, "mixerParam": false, "new_interval": true,
    "s": start, "pageSize": PAGE_SIZE,
  });
  if let Some(sort) = sort(query.sort.as_deref()) {
    body["sort"] = json!(sort);
  }
  let uuid = api
    .ctx
    .http
    .cookie("__jda")
    .and_then(|jda| jda.split('.').nth(1).map(str::to_owned))
    .filter(|u| !u.is_empty() && u != "-")
    .unwrap_or_else(|| "-1".into());
  let v = api
    .call(Call {
      function: "pc_search_searchWare",
      appid: APPID,
      path: "/api",
      sign: Some(SIGN_APP),
      body: Some(body.to_string()),
      params: vec![
        ("cthr", "1".into()),
        ("uuid", uuid),
        ("keyword", query.keyword.clone()),
      ],
      from: From::search(&query.keyword),
      form: false,
      time: true,
      order: ORDER,
    })
    .await?;
  let wares = v.list("data.wareList");
  let natural = wares.iter().filter(|w| !ad(w)).count() as u64;
  let items: Vec<Post> = wares.iter().filter_map(parse::ware).collect();
  let next = (!items.is_empty()).then(|| format!("{}.{}", number + 1, start + natural.max(2) - 1));
  Ok(Page::new(items, next))
}

fn sort(choice: Option<&str>) -> Option<&'static str> {
  match choice? {
    "sales" => Some("sort_totalsales15_desc"),
    "price-asc" => Some("sort_price_asc"),
    "price-desc" => Some("sort_price_desc"),
    "comments" => Some("sort_commentcount_desc"),
    _ => None,
  }
}

/// Explicitly marked ads, which take no place in the result offsets.
fn ad(ware: &Value) -> bool {
  ["isAdv", "isAd", "isP4p", "adFlag"].iter().any(|k| {
    let v = ware.at(k);
    v.as_bool() == Some(true)
      || v.as_i64() == Some(1)
      || v.as_str().is_some_and(|s| s == "1" || s == "true")
  })
}

fn search_page() -> From {
  From {
    origin: SEARCH_ORIGIN,
    referer: format!("{SEARCH_ORIGIN}/"),
    page: Some((format!("{SEARCH_ORIGIN}/Search"), "h5_2.1.0")),
  }
}

/// The search box's hot words (mostly promotions), all at once.
pub async fn hot(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let v = api
    .call(Call {
      function: "pc_search_hotwords",
      appid: APPID,
      path: "/api",
      sign: None,
      body: None,
      params: Vec::new(),
      from: search_page(),
      form: false,
      time: true,
      order: PLAIN_ORDER,
    })
    .await?;
  let items: Vec<Post> = v
    .list("data")
    .iter()
    .flat_map(|row| {
      row
        .as_array()
        .map_or_else(|| vec![row.clone()], Clone::clone)
    })
    .filter_map(|w| parse::hotword(&w))
    .collect();
  Ok(paging::slice(items, page))
}

/// Words the search box suggests with a keyword.
pub async fn suggest(api: &Api, keyword: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = api
    .call(Call {
      function: "pc_search_relwords",
      appid: APPID,
      path: "/api",
      sign: None,
      body: Some(json!({ "keyword": keyword }).to_string()),
      params: vec![
        ("keyword", keyword.to_owned()),
        ("num", "10".into()),
        ("rettype", "json".into()),
        ("type_name", "relsearch".into()),
      ],
      from: search_page(),
      form: false,
      time: true,
      order: PLAIN_ORDER,
    })
    .await?;
  let items = v.list("data").iter().filter_map(parse::keyword).collect();
  Ok(paging::slice(items, page))
}

/// Searches an item page suggests.
pub async fn related(api: &Api, sku: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = api
    .call(Call {
      function: "relsearch",
      appid: "item-v3",
      path: "/api",
      sign: None,
      body: Some("{}".into()),
      params: vec![
        ("skuid", sku.to_owned()),
        ("num", "10".into()),
        ("rettype", "json".into()),
        ("type_name", "relsearch".into()),
      ],
      from: From::item(sku),
      form: false,
      time: false,
      order: PLAIN_ORDER,
    })
    .await?;
  let items = v
    .list("resultKeywords")
    .iter()
    .filter_map(parse::keyword)
    .collect();
  Ok(paging::slice(items, page))
}
