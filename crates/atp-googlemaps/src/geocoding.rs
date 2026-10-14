//! Geocoding API (v3): addresses → points and back, and the ends of
//! routes, which Google takes as points or as addresses.

use atp_core::geo::{Coord, Waypoint};
use atp_core::{Data, Error, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{parse, places, refs};

/// Addresses matching `address`; `city` biases them.
pub async fn geocode(api: &Api, address: &str, city: Option<&str>) -> Result<Page<Post>> {
  let params = [
    ("address", address.to_owned()),
    (
      "components",
      city.map(|c| format!("locality:{c}")).unwrap_or_default(),
    ),
  ];
  let v = api.service("/geocode/json", &params).await?;
  Ok(Page::last(
    v.list("results")
      .iter()
      .filter_map(parse::geocoded)
      .collect(),
  ))
}

/// The address at a point; `with_pois` lists the places within 100 m
/// after it (a Nearby Search, Pro tier).
pub async fn regeo(api: &Api, location: &str, with_pois: bool) -> Result<Data> {
  let point = refs::expect_point(location, api.system())?;
  let params = [("latlng", point.lat_lng())];
  let v = api.service("/geocode/json", &params).await?;
  let address = v
    .list("results")
    .first()
    .and_then(parse::geocoded)
    .ok_or_else(|| Error::not_found(format!("Google has no address at {location}")))?;
  if !with_pois {
    return Ok(Data::Post(Box::new(address)));
  }
  let req = PageReq {
    cursor: None,
    size: 20,
  };
  let nearby = places::nearby(api, location, None, None, 100, &req).await?;
  let mut items = vec![address];
  items.extend(nearby.items);
  Ok(Data::Posts(Page::last(items)))
}

/// One end of a route: a point, or an address Google resolves itself.
pub struct End {
  pub name: String,
  pub point: Option<Coord>,
}

impl End {
  pub fn new(api: &Api, arg: &str, city: Option<&str>) -> Self {
    let name = arg.trim().to_owned();
    match refs::user_point(&name, api.system()) {
      Some(point) => Self {
        name,
        point: Some(point),
      },
      None => Self {
        name: match city {
          Some(city) => format!("{name}, {city}"),
          None => name,
        },
        point: None,
      },
    }
  }

  /// A waypoint of the Routes API.
  pub fn waypoint(&self) -> Value {
    match self.point {
      Some(point) => json!({ "location": { "latLng": refs::lat_lng(point) } }),
      None => json!({ "address": self.name }),
    }
  }

  pub fn located(&self, point: Option<Coord>) -> Waypoint {
    Waypoint {
      name: self.name.clone(),
      location: self.point.or(point),
    }
  }
}

/// A point for an end: its own, or the first geocoding match.
pub async fn locate(api: &Api, end: &End) -> Result<Coord> {
  if let Some(point) = end.point {
    return Ok(point);
  }
  let found = geocode(api, &end.name, None).await?;
  found
    .items
    .first()
    .and_then(|p| p.place.as_ref()?.location)
    .ok_or_else(|| Error::not_found(format!("Google found no place called `{}`", end.name)))
}
