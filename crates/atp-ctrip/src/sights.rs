//! Sights (景点): a city's list (`18109/json/getAttractionList`, with its
//! rankings), the sight page on `you.ctrip.com` (scores, sub-scores,
//! address, hours, photos) and its detail modules
//! (`18254/json/getPoiMoreDetail`: coordinates, tickets, facilities).

use std::time::Duration;

use atp_core::{Error, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::{Api, Site};
use crate::parse;
use crate::refs::{self, Sight};

/// Sorts of `sights --sort`, with Ctrip's ids.
pub const SORTS: &[(&str, u8)] = &[("default", 1), ("rating", 4), ("hot", 9), ("sales", 5)];
/// Sights a page of the list holds.
const STEP: u64 = 10;
const POI_TTL: Duration = Duration::from_secs(180 * 24 * 3600);

/// What `sights` narrows the list to.
#[derive(Debug, Clone, Default)]
pub struct Filter {
  pub sort: u8,
  /// Only the city itself, not the places around it.
  pub city_only: bool,
  pub level_5a: bool,
  pub free: bool,
}

/// One page of a city's sights (`district` id), with Ctrip's own filter
/// ids (`1010;5` is 5A, `60;true` free).
pub async fn list(
  api: &Api,
  district: &str,
  filter: &Filter,
  page: &PageReq,
) -> Result<Page<Post>> {
  let v = attractions(api, district, filter, page).await?;
  let number = page.number_or(1);
  let items: Vec<Post> = v
    .list("attractionList")
    .iter()
    .filter_map(|a| parse::sight_card(a.at("card")))
    .collect();
  let more = !items.is_empty() && v.bool("hasMore") == Some(true);
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

/// The list's answer (also carries the city's rankings).
pub async fn attractions(
  api: &Api,
  district: &str,
  filter: &Filter,
  page: &PageReq,
) -> Result<Value> {
  let mut items = Vec::new();
  if filter.city_only {
    items.push(format!("4;{district}"));
  }
  if filter.level_5a {
    items.push("1010;5".into());
  }
  if filter.free {
    items.push("60;true".into());
  }
  // The PC site's system code: under the H5 one (`09`) the list leaves
  // out the city's rankings.
  let mut head = api.head();
  head["syscode"] = "999".into();
  let body = json!({
    "scene": "online",
    "districtId": district.parse::<u64>().map_err(|_| Error::input("bad district id"))?,
    "index": page.number_or(1),
    "count": STEP,
    "sortType": filter.sort.max(1),
    "filter": { "filterItems": items },
    "returnModuleType": "all",
    "head": head
  });
  api
    .soa("18109/json/getAttractionList", Site::You, body)
    .await
}

/// A sight from its page (`read`): scores, sub-scores, address, hours,
/// phones, introduction and photos.
pub async fn read(api: &Api, sight: &Sight) -> Result<Post> {
  let id = sight_id(api, sight).await?;
  let url = refs::sight_page(&id, sight.district.as_deref());
  let html = api.page(&url, Site::You).await?;
  let state = parse::next_data(&html)
    .map(|d| d.at("props.pageProps.initialState").clone())
    .filter(|s| s.at("poiDetail").is_object())
    .ok_or_else(|| Error::not_found(format!("Ctrip shows no sight {id}")))?;
  let post = parse::sight_page(&state)
    .ok_or_else(|| Error::upstream(format!("no sight data in Ctrip's page of {id}")))?;
  if let Some(poi) = post.extra.get("poi_id").and_then(Value::as_str) {
    remember(api, &id, poi);
  }
  Ok(post)
}

/// `sight`: the page, plus coordinates, ticket notes and facilities.
pub async fn detail(api: &Api, sight: &Sight) -> Result<Post> {
  let mut post = read(api, sight).await?;
  let poi = post
    .extra
    .get("poi_id")
    .and_then(Value::as_str)
    .map(str::to_owned);
  if let Some(poi) = poi {
    let more = poi_detail(api, &poi).await?;
    parse::sight_more(&mut post, &more);
  }
  Ok(post)
}

async fn poi_detail(api: &Api, poi: &str) -> Result<Value> {
  let body = json!({ "poiId": refs::number(poi)?, "scene": "basic" });
  api
    .soa("18254/json/getPoiMoreDetail", Site::Mobile, body)
    .await
}

/// The sight id, looked up by POI id when only that is known.
async fn sight_id(api: &Api, sight: &Sight) -> Result<String> {
  if let Some(id) = &sight.sight {
    return Ok(id.clone());
  }
  let poi = sight
    .poi
    .as_deref()
    .ok_or_else(|| Error::input("give a sight id or link"))?;
  let v = poi_detail(api, poi).await?;
  v.list("templateList")
    .iter()
    .flat_map(|t| t.list("moduleList"))
    .find_map(|m| m.str("poiBasicModule.sightId"))
    .ok_or_else(|| Error::not_found(format!("Ctrip knows no sight for POI {poi}")))
}

/// The POI id reviews want: given, remembered, or read from the sight's
/// page (once).
pub async fn poi_id(api: &Api, sight: &Sight) -> Result<String> {
  if let Some(poi) = &sight.poi {
    return Ok(poi.clone());
  }
  let id = sight.sight.as_deref().unwrap_or_default();
  if let Some(poi) = api.ctx.store.cache_get::<String>(&cache_key(id), POI_TTL) {
    return Ok(poi);
  }
  let post = read(api, sight).await?;
  post
    .extra
    .get("poi_id")
    .and_then(Value::as_str)
    .map(str::to_owned)
    .ok_or_else(|| Error::upstream(format!("Ctrip's page of sight {id} names no POI")))
}

fn remember(api: &Api, sight: &str, poi: &str) {
  api.ctx.store.cache_put(&cache_key(sight), &poi);
}

fn cache_key(sight: &str) -> String {
  format!("ctrip-sight-poi-{sight}")
}
