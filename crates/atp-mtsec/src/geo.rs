//! Meituan's city and area lookups on `apimobile.meituan.com`, open to
//! visitors: the city, district and business area at a point (with both
//! Meituan's and Dianping's city ids), where an IP address is, and a
//! city's districts and business areas.

use atp_core::{Collection, Ctx, Error, Result, Value, ValueExt, json};

use crate::guard::{self, Call, MEITUAN};

const API: &str = "https://apimobile.meituan.com";
const FROM: &str = "https://www.meituan.com/";

/// The place at a point (WGS-84 or GCJ-02; Meituan does not say which it
/// expects, and the difference rarely changes the city).
pub async fn locate(ctx: &Ctx, lat: f64, lng: f64) -> Result<Value> {
  let call = Call::xhr(format!("{API}/group/v1/city/latlng/{lat},{lng}"), FROM).query("tag", 0);
  let v = guard::json(ctx, &MEITUAN, call).await?;
  let d = v.at("data");
  if !d.is_object() {
    return Err(Error::not_found(format!(
      "Meituan knows no city at {lat},{lng}"
    )));
  }
  Ok(compact(json!({
    "city": d.str("city"),
    "city_id": d.i64("id"),
    "dianping_city_id": d.i64("dpCityId"),
    "pinyin": d.str("cityPinyin"),
    "province": d.str("province"),
    "district": d.str("district"),
    "area": d.str("areaName"),
    "area_id": d.i64("area"),
    "address": d.str("detail"),
    "served": d.bool("isOpen"),
    "lat": d.f64("lat"),
    "lng": d.f64("lng"),
  })))
}

/// Where an IP address is; the one atp connects from without one.
pub async fn ip(ctx: &Ctx, ip: Option<&str>) -> Result<Value> {
  let call = Call::xhr(format!("{API}/locate/v2/ip/loc"), FROM)
    .query("rgeo", "true")
    .query("ip", ip.unwrap_or_default());
  let v = guard::json(ctx, &MEITUAN, call).await?;
  if let Some(message) = v.str("error.message") {
    let who = ip.unwrap_or("this connection's address");
    return Err(Error::not_found(format!(
      "Meituan cannot place {who}: {message}"
    )));
  }
  let d = v.at("data");
  Ok(compact(json!({
    "ip": ip,
    "country": d.str("rgeo.country"),
    "province": d.str("rgeo.province"),
    "city": d.str("rgeo.city"),
    "district": d.str("rgeo.district"),
    "adcode": d.str("rgeo.adcode"),
    "lat": d.f64("lat"),
    "lng": d.f64("lng"),
  })))
}

/// A city's districts, or the business areas of one of them (by id or
/// name).
pub async fn areas(ctx: &Ctx, city: &str, district: Option<&str>) -> Result<Vec<Collection>> {
  let call = Call::xhr(format!("{API}/group/v2/area/list"), FROM).query("cityId", city);
  let v = guard::json(ctx, &MEITUAN, call).await?;
  let districts = v.list("data.areasinfo");
  if districts.is_empty() {
    return Err(Error::not_found(format!(
      "Meituan has no areas for city {city}"
    )));
  }
  let Some(wanted) = district else {
    return Ok(districts.iter().map(|d| area(d, "district")).collect());
  };
  let found = districts
    .iter()
    .find(|d| d.str("id").as_deref() == Some(wanted) || d.str("name").as_deref() == Some(wanted))
    .ok_or_else(|| Error::not_found(format!("no district {wanted} in city {city}")))?;
  let id = found.i64("id");
  Ok(
    v.list("data.subareasinfo")
      .iter()
      .filter(|a| a.i64("district") == id)
      .map(|a| area(a, "area"))
      .collect(),
  )
}

fn area(v: &Value, kind: &str) -> Collection {
  let mut collection = Collection {
    id: v.str("id").unwrap_or_default(),
    kind: kind.into(),
    name: v.str("name").unwrap_or_default(),
    items: (kind == "district").then(|| v.list("subareas").len() as u64),
    ..Collection::default()
  };
  for key in ["lat", "lng", "slug"] {
    if let Some(value) = v.get(key).filter(|x| !x.is_null()) {
      collection.extra.insert(key.into(), value.clone());
    }
  }
  collection
}

/// The object without its empty fields.
fn compact(v: Value) -> Value {
  match v {
    Value::Object(map) => Value::Object(map.into_iter().filter(|(_, x)| !x.is_null()).collect()),
    other => other,
  }
}
