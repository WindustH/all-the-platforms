//! Geocoding v3 and reverse geocoding v3, IP location, and turning a place
//! argument (`LNG,LAT` or a name) into a point.

use atp_core::geo::{Coord, Place, Waypoint};
use atp_core::{Data, Error, Page, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::places::returned;
use crate::{parse, refs};

/// One end of a route or distance, as a point in the wire system.
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
  if let Some(location) = refs::point(arg, api.system(), api.wire()) {
    return Ok(End {
      name: arg.trim().to_owned(),
      location,
    });
  }
  let v = geo(api, arg, city).await?;
  let location = refs::located(v.at("result.location"), api.wire())
    .ok_or_else(|| Error::not_found(format!("Baidu found no place called `{arg}`")))?;
  Ok(End {
    name: arg.trim().to_owned(),
    location,
  })
}

async fn geo(api: &Api, address: &str, city: Option<&str>) -> Result<Value> {
  let params = [
    ("address", address.to_owned()),
    ("city", city.unwrap_or_default().to_owned()),
    ("ret_coordtype", returned(api.wire())),
  ];
  api.get("/geocoding/v3/", &params).await
}

/// Where an address is: Baidu answers with one match.
pub async fn geocode(api: &Api, address: &str, city: Option<&str>) -> Result<Page<Post>> {
  let v = geo(api, address, city).await?;
  let r = v.at("result");
  let Some(location) = refs::located(r.at("location"), api.wire()) else {
    return Ok(Page::last(Vec::new()));
  };
  let place = Place {
    location: Some(location),
    address: Some(address.to_owned()),
    category: r.str("level"),
    city: city.map(str::to_owned),
    ..Place::default()
  };
  let mut post = address_post(place, r);
  for key in ["precise", "confidence", "comprehension"] {
    if let Some(n) = r.i64(key) {
      post.extra.insert(key.into(), n.into());
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
  let wire = api.wire();
  let point = refs::expect_point(location, api.system(), wire)?;
  let params = [
    ("location", point.lat_lng()),
    ("coordtype", refs::ll(wire).to_owned()),
    ("ret_coordtype", returned(wire)),
    (
      "extensions_poi",
      if with_pois { "1" } else { "0" }.to_owned(),
    ),
  ];
  let v = api.get("/reverse_geocoding/v3/", &params).await?;
  let r = v.at("result");
  let c = r.at("addressComponent");
  let place = Place {
    location: refs::located(r.at("location"), wire).or(Some(point)),
    address: r.str("formatted_address"),
    country: c.str("country"),
    province: c.str("province"),
    city: c.str("city"),
    district: c.str("district"),
    adcode: c.str("adcode"),
    ..Place::default()
  };
  let mut address = address_post(place, r);
  for (key, value) in [
    ("town", c.str("town")),
    ("street", c.str("street")),
    ("number", c.str("street_number")),
    ("business", r.str("business")),
    ("description", r.str("sematic_description")),
  ] {
    if let Some(value) = value {
      address.extra.insert(key.into(), value.into());
    }
  }
  if !with_pois {
    return Ok(Data::Post(Box::new(address)));
  }
  let mut items = vec![address];
  items.extend(r.list("pois").iter().filter_map(|p| parse::nearby(p, wire)));
  Ok(Data::Posts(Page::last(items)))
}

/// Where an IP is (the caller's when `ip` is empty): the city's centre.
pub async fn ip(api: &Api, ip: Option<&str>) -> Result<Value> {
  let wire = api.wire();
  let coor = match wire {
    atp_core::geo::System::Bd09 => "bd09ll",
    _ => "gcj02",
  };
  let params = [
    ("ip", ip.unwrap_or_default().to_owned()),
    ("coor", coor.to_owned()),
  ];
  let v = api.get("/location/ip", &params).await?;
  let c = v.at("content");
  let d = c.at("address_detail");
  Ok(json!({
    "ip": ip,
    "address": c.str("address"),
    "province": d.str("province"),
    "city": d.str("city"),
    "district": d.str("district"),
    "adcode": d.str("adcode"),
    "location": refs::located(c.at("point"), wire),
  }))
}
