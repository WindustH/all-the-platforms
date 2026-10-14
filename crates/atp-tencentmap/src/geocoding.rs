//! Geocoding and reverse geocoding (`/ws/geocoder/v1/`), IP location, and
//! turning a place argument (`LNG,LAT` or a name) into a point.

use atp_core::geo::{Coord, Place, Waypoint};
use atp_core::{Data, Error, Page, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{parse, refs};

/// One end of a route or distance, as a GCJ-02 point.
pub struct End {
  pub name: String,
  pub location: Coord,
}

impl End {
  pub fn waypoint(&self) -> Waypoint {
    Waypoint {
      name: self.name.clone(),
      location: Some(self.location),
    }
  }
}

/// `LNG,LAT` as given, or where geocoding puts a name.
pub async fn end(api: &Api, arg: &str, city: Option<&str>) -> Result<End> {
  if let Some(location) = refs::point(arg, api.system()) {
    return Ok(End {
      name: arg.trim().to_owned(),
      location,
    });
  }
  let v = geo(api, arg, city).await?;
  let location = refs::located(v.at("result.location"))
    .ok_or_else(|| Error::not_found(format!("Tencent found no place called `{arg}`")))?;
  Ok(End {
    name: arg.trim().to_owned(),
    location,
  })
}

/// Geocoding wants the city inside the address; `policy=1` lets it go
/// without one.
async fn geo(api: &Api, address: &str, city: Option<&str>) -> Result<Value> {
  let address = match city {
    Some(city) if !address.contains(city) => format!("{city}{address}"),
    _ => address.to_owned(),
  };
  let params = [("address", address), ("policy", "1".to_owned())];
  api.get("/ws/geocoder/v1/", &params).await
}

/// Where an address is: Tencent answers with one match.
pub async fn geocode(api: &Api, address: &str, city: Option<&str>) -> Result<Page<Post>> {
  let v = geo(api, address, city).await?;
  let r = v.at("result");
  let Some(location) = refs::located(r.at("location")) else {
    return Ok(Page::last(Vec::new()));
  };
  let c = r.at("address_components");
  let place = Place {
    location: Some(location),
    address: Some(address.to_owned()),
    province: c.str("province"),
    city: c.str("city"),
    district: c.str("district"),
    adcode: r.str("ad_info.adcode"),
    ..Place::default()
  };
  let mut post = address_post(place, r);
  for (key, value) in [
    ("street", c.str("street")),
    ("number", c.str("street_number")),
    ("level", r.str("level")),
    ("reliability", r.str("reliability")),
  ] {
    if let Some(value) = value {
      post.extra.insert(key.into(), value.into());
    }
  }
  Ok(Page::last(vec![post]))
}

fn address_post(place: Place, raw: &Value) -> Post {
  Post {
    id: place.location.map(|c| c.lng_lat()).unwrap_or_default(),
    kind: "address".into(),
    title: place.address.clone(),
    place: Some(Box::new(place)),
    raw: Some(raw.clone()),
    ..Post::default()
  }
}

/// The address at a point; `with_pois` lists the places nearby after it.
pub async fn regeo(api: &Api, location: &str, with_pois: bool) -> Result<Data> {
  let point = refs::expect_point(location, api.system())?;
  let params = [
    ("location", point.lat_lng()),
    ("get_poi", if with_pois { "1" } else { "0" }.to_owned()),
  ];
  let v = api.get("/ws/geocoder/v1/", &params).await?;
  let r = v.at("result");
  let c = r.at("address_component");
  let place = Place {
    location: Some(point),
    address: r.first_str(&["formatted_addresses.standard_address", "address"]),
    country: c.str("nation"),
    province: c.str("province"),
    city: c.str("city"),
    district: c.str("district"),
    adcode: r.str("ad_info.adcode"),
    ..Place::default()
  };
  let mut address = address_post(place, r);
  for (key, value) in [
    ("street", c.str("street")),
    ("number", c.str("street_number")),
    ("description", r.str("formatted_addresses.recommend")),
    ("town", r.str("address_reference.town.title")),
  ] {
    if let Some(value) = value {
      address.extra.insert(key.into(), value.into());
    }
  }
  if !with_pois {
    return Ok(Data::Post(Box::new(address)));
  }
  let mut items = vec![address];
  items.extend(r.list("pois").iter().filter_map(parse::place));
  Ok(Data::Posts(Page::last(items)))
}

/// Where an IP is (the caller's when `ip` is empty): its district's centre.
pub async fn ip(api: &Api, ip: Option<&str>) -> Result<Value> {
  let v = api
    .get(
      "/ws/location/v1/ip",
      &[("ip", ip.unwrap_or_default().to_owned())],
    )
    .await?;
  let r = v.at("result");
  let ad = r.at("ad_info");
  Ok(json!({
    "ip": r.str("ip"),
    "country": ad.str("nation"),
    "province": ad.str("province"),
    "city": ad.str("city"),
    "district": ad.str("district"),
    "adcode": ad.str("adcode"),
    "location": refs::located(r.at("location")),
  }))
}
