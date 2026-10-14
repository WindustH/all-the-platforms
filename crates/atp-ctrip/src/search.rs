//! `search`: Ctrip's global search (`26872/search`), which answers places
//! of every kind (sights, hotels, restaurants, districts, tours) with
//! their web links; and `suggest`, the destination and hotel suggestions
//! of the booking forms (`21881/json/gaHotelSearchEngine`).

use atp_core::{Error, Page, PageReq, Post, Query, Result, Value, ValueExt, json};

use crate::api::{Api, Site};
use crate::parse;

/// Results a page of the global search holds.
const STEP: u64 = 10;

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let number = page.number_or(1);
  let v = global(api, &query.keyword, number).await?;
  let entries = v.list("data");
  let items: Vec<Post> = entries.iter().filter_map(parse::search_item).collect();
  let total = v.u64("total").unwrap_or_default();
  let more = !entries.is_empty() && number * STEP < total;
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

/// One page of the global search's answer.
pub async fn global(api: &Api, keyword: &str, number: u64) -> Result<Value> {
  if keyword.trim().is_empty() {
    return Err(Error::input("give something to search for"));
  }
  let body = json!({
    "action": "online", "source": "globalonline", "keyword": keyword,
    "pagenum": number, "pagesize": STEP
  });
  api.soa("26872/search", Site::Mobile, body).await
}

/// Destinations (`dest`) or hotel-search places (`hotel`) for a keyword.
pub async fn suggest(api: &Api, keyword: &str, hotels: bool) -> Result<Page<Post>> {
  let v = suggestions(api, keyword, hotels).await?;
  let items = v
    .list("Response.searchResults")
    .iter()
    .filter_map(parse::suggestion)
    .collect();
  Ok(Page::last(items))
}

/// The raw suggestions for a keyword.
pub async fn suggestions(api: &Api, keyword: &str, hotels: bool) -> Result<Value> {
  if keyword.trim().is_empty() {
    return Err(Error::input("give a place to look up"));
  }
  let body = json!({
    "keyword": keyword,
    "searchType": if hotels { "H" } else { "D" },
    "platform": "online",
    "pageID": "102001",
    "head": {
      "Locale": "zh-CN", "LocaleController": "zh_cn", "Currency": "CNY", "PageId": "102001",
      "clientID": api.cid(), "group": "ctrip", "Frontend": { "sessionID": 1, "pvid": 1 },
      "HotelExtension": { "group": "CTRIP", "WebpSupport": false }
    }
  });
  api
    .soa("21881/json/gaHotelSearchEngine", Site::Mobile, body)
    .await
}
