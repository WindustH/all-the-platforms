//! Addresses and coordinates: geocoding, reverse geocoding, conversion from
//! other coordinate systems, IP location, and turning a place argument
//! (`LNG,LAT` or a name) into a point.

use atp_core::geo::{Coord, Place, System, Waypoint};
use atp_core::{Data, Error, Page, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{parse, refs};

/// One end of a route or distance, resolved to a point.
pub struct End {
  pub name: String,
  pub location: Coord,
  pub citycode: Option<String>,
}

impl End {
  pub fn waypoint(&self) -> Waypoint {
    Waypoint {
      name: self.name.clone(),
      location: Some(self.location),
    }
  }

  /// Amap's `lng,lat`.
  pub fn lng_lat(&self) -> String {
    self.location.lng_lat()
  }
}

/// `LNG,LAT` as given, or the first geocoding match of a name.
pub async fn end(api: &Api, arg: &str, city: Option<&str>) -> Result<End> {
  if let Some(c) = Coord::parse(arg, api.system()) {
    return Ok(End {
      name: arg.trim().to_owned(),
      location: c.to(System::Gcj02),
      citycode: None,
    });
  }
  let v = geo(api, arg, city).await?;
  let first = v.at("geocodes.0");
  let location = first
    .str("location")
    .as_deref()
    .and_then(parse::coord)
    .ok_or_else(|| Error::not_found(format!("Amap found no place called `{arg}`")))?;
  Ok(End {
    name: first
      .str("formatted_address")
      .unwrap_or_else(|| arg.to_owned()),
    location,
    citycode: first.str("citycode"),
  })
}

/// The city code of an end, looked up by reverse geocoding when it came
/// as coordinates.
pub async fn citycode(api: &Api, end: &End) -> Result<String> {
  if let Some(code) = &end.citycode {
    return Ok(code.clone());
  }
  let v = api
    .get("/v3/geocode/regeo", &[("location", end.lng_lat())])
    .await?;
  v.str("regeocode.addressComponent.citycode")
    .ok_or_else(|| Error::not_found(format!("no city at {}", end.lng_lat())))
}

async fn geo(api: &Api, address: &str, city: Option<&str>) -> Result<Value> {
  let params = [
    ("address", address.to_owned()),
    ("city", city.unwrap_or_default().to_owned()),
  ];
  api.get("/v3/geocode/geo", &params).await
}

/// Addresses matching `address`, as places.
pub async fn geocode(api: &Api, address: &str, city: Option<&str>) -> Result<Page<Post>> {
  let v = geo(api, address, city).await?;
  Ok(Page::last(
    v.list("geocodes").iter().filter_map(geocoded).collect(),
  ))
}

fn geocoded(g: &Value) -> Option<Post> {
  let location = g.str("location").as_deref().and_then(parse::coord)?;
  let place = Place {
    location: Some(location),
    address: g.str("formatted_address"),
    province: g.str("province"),
    city: g.str("city"),
    district: g.str("district"),
    adcode: g.str("adcode"),
    ..Place::default()
  };
  let mut post = address_post(location, place, g);
  for (key, path) in [("level", "level"), ("citycode", "citycode")] {
    if let Some(value) = g.str(path) {
      post.extra.insert(key.into(), value.into());
    }
  }
  Some(post)
}

/// An address as a post: its point as id, its text as title.
fn address_post(location: Coord, place: Place, raw: &Value) -> Post {
  Post {
    id: location.lng_lat(),
    kind: "address".into(),
    title: place.address.clone(),
    place: Some(Box::new(place)),
    raw: Some(raw.clone()),
    ..Post::default()
  }
}

/// The address at a point; `with_pois` lists the places nearby after it.
pub async fn regeo(api: &Api, location: &str, with_pois: bool) -> Result<Data> {
  let point = refs::expect_location(location, api.system())?;
  let params = [
    ("location", point.clone()),
    (
      "extensions",
      if with_pois { "all" } else { "base" }.to_owned(),
    ),
  ];
  let v = api.get("/v3/geocode/regeo", &params).await?;
  let r = v.at("regeocode");
  let address = reverse(r, &point);
  if !with_pois {
    return Ok(Data::Post(Box::new(address)));
  }
  let mut items = vec![address];
  items.extend(r.list("pois").iter().filter_map(parse::nearby_poi));
  Ok(Data::Posts(Page::last(items)))
}

fn reverse(r: &Value, point: &str) -> Post {
  let c = r.at("addressComponent");
  let location = parse::coord(point).unwrap_or(Coord::new(0.0, 0.0, System::Gcj02));
  let place = Place {
    location: Some(location),
    address: r.str("formatted_address"),
    country: c.str("country"),
    province: c.str("province"),
    city: c.str("city"),
    district: c.str("district"),
    adcode: c.str("adcode"),
    ..Place::default()
  };
  let mut post = address_post(location, place, r);
  for (key, path) in [
    ("township", "township"),
    ("street", "streetNumber.street"),
    ("number", "streetNumber.number"),
    ("citycode", "citycode"),
  ] {
    if let Some(value) = c.str(path) {
      post.extra.insert(key.into(), value.into());
    }
  }
  let areas: Vec<String> = c
    .list("businessAreas")
    .iter()
    .filter_map(|a| a.str("name"))
    .collect();
  if !areas.is_empty() {
    post.extra.insert("business_areas".into(), areas.into());
  }
  post
}

/// Coordinates from GPS (WGS-84), Baidu or Mapbar into Amap's GCJ-02.
pub async fn convert(api: &Api, coords: &str, from: &str) -> Result<Value> {
  let points: Vec<String> = coords
    .split(['|', ';'])
    .map(|c| refs::expect_location(c, System::Gcj02))
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
  let corners: Vec<Coord> = v
    .str("rectangle")
    .unwrap_or_default()
    .split(';')
    .filter_map(parse::coord)
    .collect();
  Ok(json!({
    "ip": ip,
    "province": v.str("province"),
    "city": v.str("city"),
    "adcode": v.str("adcode"),
    "rectangle": corners,
  }))
}
