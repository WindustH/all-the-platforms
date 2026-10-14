//! Addresses and coordinates: geocoding, reverse geocoding, conversion from
//! other coordinate systems, IP location, and turning a place argument
//! (`lng,lat` or a name) into a location.

use atp_core::{Error, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs;

/// A place resolved to a location.
pub struct Place {
  pub name: String,
  pub location: String,
  pub citycode: Option<String>,
}

/// `lng,lat` as given, or the first geocoding match of a name.
pub async fn place(api: &Api, arg: &str, city: Option<&str>) -> Result<Place> {
  if let Some(location) = refs::location(arg) {
    return Ok(Place {
      name: location.clone(),
      location,
      citycode: None,
    });
  }
  let v = geo(api, arg, city).await?;
  let first = v.at("geocodes.0");
  let location = first
    .str("location")
    .ok_or_else(|| Error::not_found(format!("Amap found no place called `{arg}`")))?;
  Ok(Place {
    name: first
      .str("formatted_address")
      .unwrap_or_else(|| arg.to_owned()),
    location,
    citycode: first.str("citycode"),
  })
}

/// The city code of a place, looked up by reverse geocoding when the place
/// came as coordinates.
pub async fn citycode(api: &Api, place: &Place) -> Result<String> {
  if let Some(code) = &place.citycode {
    return Ok(code.clone());
  }
  let v = api
    .get("/v3/geocode/regeo", &[("location", place.location.clone())])
    .await?;
  v.str("regeocode.addressComponent.citycode")
    .ok_or_else(|| Error::not_found(format!("no city at {}", place.location)))
}

async fn geo(api: &Api, address: &str, city: Option<&str>) -> Result<Value> {
  let params = [
    ("address", address.to_owned()),
    ("city", city.unwrap_or_default().to_owned()),
  ];
  api.get("/v3/geocode/geo", &params).await
}

pub async fn geocode(api: &Api, address: &str, city: Option<&str>) -> Result<Value> {
  let v = geo(api, address, city).await?;
  let results: Vec<Value> = v
    .list("geocodes")
    .iter()
    .map(|g| {
      json!({
        "address": g.str("formatted_address"),
        "location": g.str("location"),
        "level": g.str("level"),
        "province": g.str("province"),
        "city": g.str("city"),
        "district": g.str("district"),
        "adcode": g.str("adcode"),
        "citycode": g.str("citycode"),
      })
    })
    .collect();
  Ok(json!({ "query": address, "results": results }))
}

/// The address at a location; `with_pois` adds nearby places and roads.
pub async fn regeo(api: &Api, location: &str, with_pois: bool) -> Result<Value> {
  let params = [
    ("location", refs::expect_location(location)?),
    (
      "extensions",
      if with_pois { "all" } else { "base" }.to_owned(),
    ),
  ];
  let v = api.get("/v3/geocode/regeo", &params).await?;
  let r = v.at("regeocode");
  let c = r.at("addressComponent");
  let areas: Vec<String> = c
    .list("businessAreas")
    .iter()
    .filter_map(|a| a.str("name"))
    .collect();
  let pois: Vec<Value> = r
    .list("pois")
    .iter()
    .map(|p| json!({ "id": p.str("id"), "name": p.str("name"), "type": p.str("type"), "distance_m": p.str("distance") }))
    .collect();
  Ok(json!({
    "location": location,
    "address": r.str("formatted_address"),
    "province": c.str("province"),
    "city": c.str("city"),
    "district": c.str("district"),
    "township": c.str("township"),
    "street": c.str("streetNumber.street"),
    "number": c.str("streetNumber.number"),
    "adcode": c.str("adcode"),
    "citycode": c.str("citycode"),
    "business_areas": areas,
    "pois": pois,
  }))
}

/// Coordinates from GPS (WGS-84), Baidu or Mapbar into Amap's GCJ-02.
pub async fn convert(api: &Api, coords: &str, from: &str) -> Result<Value> {
  let points: Vec<String> = coords
    .split(['|', ';'])
    .map(refs::expect_location)
    .collect::<Result<_>>()?;
  let params = [
    ("locations", points.join("|")),
    ("coordsys", from.to_owned()),
  ];
  let v = api.get("/v3/assistant/coordinate/convert", &params).await?;
  let converted: Vec<String> = v
    .str("locations")
    .unwrap_or_default()
    .split(';')
    .map(str::to_owned)
    .collect();
  Ok(json!({ "from": from, "input": points, "gcj02": converted }))
}

/// Where an IP is (the caller's when `ip` is empty); mainland China only.
pub async fn ip(api: &Api, ip: Option<&str>) -> Result<Value> {
  let v = api
    .get("/v3/ip", &[("ip", ip.unwrap_or_default().to_owned())])
    .await?;
  Ok(json!({
    "ip": ip,
    "province": v.str("province"),
    "city": v.str("city"),
    "adcode": v.str("adcode"),
    "rectangle": v.str("rectangle"),
  }))
}
