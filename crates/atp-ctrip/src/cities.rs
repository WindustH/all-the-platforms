//! Cities. Ctrip numbers places twice: the travel-guide side (sights,
//! rankings, the community) by district id (杭州 14), the hotel side by
//! city id (杭州 17). Commands take a name or the id they want; atp looks
//! names up once and remembers them.

use std::time::Duration;

use atp_core::{Collection, Error, Page, Result, Value, ValueExt, credentials};

use crate::api::Api;
use crate::parse;
use crate::search;

/// Where `hot` and the city feed look without `CTRIP_CITY`: 上海.
const DEFAULT_DISTRICT: &str = "2";
const TTL: Duration = Duration::from_secs(90 * 24 * 3600);

/// The district id of `arg` (a name or the id), else of `CTRIP_CITY`,
/// else Shanghai's.
pub async fn district(api: &Api, arg: Option<&str>) -> Result<String> {
  let arg = arg
    .map(str::to_owned)
    .or_else(|| credentials::env("CTRIP_CITY"))
    .unwrap_or_else(|| DEFAULT_DISTRICT.to_owned());
  let name = arg.trim();
  if is_id(name) {
    return Ok(name.to_owned());
  }
  let key = format!("ctrip-district-{name}");
  if let Some(id) = api.ctx.store.cache_get::<String>(&key, TTL) {
    return Ok(id);
  }
  let v = search::global(api, name, 1).await?;
  let id = best(v.list("data"), name, |p| {
    p.str("type").as_deref() == Some("district")
  })
  .and_then(|p| p.str("id"))
  .ok_or_else(|| Error::not_found(format!("Ctrip knows no city or district {name}")))?;
  api.ctx.store.cache_put(&key, &id);
  Ok(id)
}

/// The hotel-side city id of `arg` (a name or the id).
pub async fn city(api: &Api, arg: &str) -> Result<String> {
  let name = arg.trim();
  if is_id(name) {
    return Ok(name.to_owned());
  }
  let key = format!("ctrip-city-{name}");
  if let Some(id) = api.ctx.store.cache_get::<String>(&key, TTL) {
    return Ok(id);
  }
  let v = search::suggestions(api, name, false).await?;
  let id = best(v.list("Response.searchResults"), name, |p| {
    p.str("type").as_deref() == Some("City")
  })
  .and_then(|p| p.str("cityId"))
  .filter(|id| id != "0")
  .ok_or_else(|| Error::not_found(format!("Ctrip knows no city {name}")))?;
  api.ctx.store.cache_put(&key, &id);
  Ok(id)
}

/// `cities NAME`: places of that name with both ids.
pub async fn cities(api: &Api, name: &str) -> Result<Page<Collection>> {
  let v = search::global(api, name, 1).await?;
  let items = v
    .list("data")
    .iter()
    .filter(|p| p.str("type").as_deref() == Some("district"))
    .filter_map(place)
    .collect();
  Ok(Page::last(items))
}

fn place(v: &Value) -> Option<Collection> {
  let id = v.str("id")?;
  let mut c = Collection {
    kind: v
      .str("childType")
      .map(|k| k.to_ascii_lowercase())
      .unwrap_or_else(|| "district".into()),
    name: v.str("word").unwrap_or_default(),
    description: v.str("districtName").map(|d| format!("in {d}")),
    url: v.str("url").map(|u| parse::absolute(&u)),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  let extra = &mut c.extra;
  parse::put(extra, "district_id", Some(id.clone().into()));
  parse::put(
    extra,
    "city_id",
    v.str("cityId").filter(|c| c != "0").map(Value::from),
  );
  parse::put(
    extra,
    "parent_district_id",
    v.str("districtId").filter(|d| *d != id).map(Value::from),
  );
  parse::location(extra, v, "lat", "lon", None);
  c.id = id;
  Some(c)
}

/// The entry named exactly `name`, else the first one `wanted`.
fn best<'a>(list: &'a [Value], name: &str, wanted: impl Fn(&Value) -> bool) -> Option<&'a Value> {
  let named = |p: &Value| {
    p.str("word")
      .is_some_and(|w| w == name || w.trim_end_matches(['市', '县', '区']) == name)
  };
  list
    .iter()
    .find(|p| wanted(p) && named(p))
    .or_else(|| list.iter().find(|p| wanted(p)))
}

fn is_id(arg: &str) -> bool {
  !arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit())
}
