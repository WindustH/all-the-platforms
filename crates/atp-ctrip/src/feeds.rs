//! `feed`: the community square (`32956/json/searchSquareFeeds`, 推荐) or
//! a city's notes (`32956/json/searchCityFeeds`, 同城, the city in
//! `CTRIP_CITY`), cursor `pageKey`.

use atp_core::{Page, PageReq, Post, Result, json};

use crate::api::{Api, Site};
use crate::{cities, parse};

/// `feed --type`.
pub const KINDS: &[&str] = &["recommend", "city"];
/// Entries a page holds.
const STEP: u64 = 10;

pub async fn feed(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let para = json!({ "pageSize": STEP, "pageKey": page.cursor.clone().unwrap_or_default() });
  let (path, body) = match kind {
    Some("city") => {
      let district = cities::district(api, None).await?;
      let body = json!({ "pagePara": para, "districtId": district.parse::<u64>().unwrap_or(2) });
      ("32956/json/searchCityFeeds", body)
    }
    _ => (
      "32956/json/searchSquareFeeds",
      json!({ "pagePara": para, "channel": "square" }),
    ),
  };
  let v = api.soa(path, Site::Mobile, body).await?;
  Ok(parse::mix_page(&v, "feeds"))
}
