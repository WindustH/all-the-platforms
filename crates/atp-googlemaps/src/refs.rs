//! What users give: place ids (bare, `places/ID`, or inside a Google Maps
//! link) and `LNG,LAT` points, and the points Google takes and returns:
//! GCJ-02 inside mainland China, WGS-84 elsewhere.

use atp_core::geo::{Coord, System};
use atp_core::{Error, Result, Value, json};
use url::Url;

/// A place id from `ChIJ…`, `places/ChIJ…`, a link with `place_id:ID`
/// (ours) or `query_place_id=ID`.
pub fn place_id(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let id = match Url::parse(arg) {
    Ok(url) => from_link(&url),
    Err(_) => Some(arg.strip_prefix("places/").unwrap_or(arg).to_owned()),
  };
  id.filter(|id| {
    id.len() >= 10
      && id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
  })
  .ok_or_else(|| {
    Error::input(format!(
      "not a Google place: `{arg}` (expected a place id like ChIJN1t_tDeuEmsRUsoyG83frY4)"
    ))
  })
}

fn from_link(url: &Url) -> Option<String> {
  url.query_pairs().find_map(|(k, v)| match k.as_ref() {
    "query_place_id" => Some(v.into_owned()),
    "q" | "query" => v.strip_prefix("place_id:").map(str::to_owned),
    _ => None,
  })
}

/// The link `read` takes back.
pub fn place_url(id: &str) -> String {
  format!("https://www.google.com/maps/place/?q=place_id:{id}")
}

/// A point as Google takes it: GCJ-02 inside mainland China, WGS-84
/// elsewhere.
pub fn to_google(c: Coord) -> Coord {
  if c.in_mainland_china() {
    c.to(System::Gcj02)
  } else {
    c.to(System::Wgs84)
  }
}

/// A point Google returned, tagged with the system it is in.
pub fn from_google(lng: f64, lat: f64) -> Coord {
  let wgs = Coord::new(lng, lat, System::Wgs84);
  if wgs.in_mainland_china() {
    Coord::new(lng, lat, System::Gcj02)
  } else {
    wgs
  }
}

/// `{latitude, longitude}` (the newer APIs) or `{lat, lng}` (web services).
pub fn point(v: &Value) -> Option<Coord> {
  let lat = v.get("latitude").or_else(|| v.get("lat"))?.as_f64()?;
  let lng = v.get("longitude").or_else(|| v.get("lng"))?.as_f64()?;
  Some(from_google(lng, lat))
}

/// `LNG,LAT` in the `--coord` system, as Google takes it.
pub fn user_point(arg: &str, system: System) -> Option<Coord> {
  Coord::parse(arg, system).map(to_google)
}

pub fn expect_point(arg: &str, system: System) -> Result<Coord> {
  Coord::expect(arg, system).map(to_google)
}

/// `{latitude, longitude}` of the newer APIs.
pub fn lat_lng(c: Coord) -> Value {
  json!({ "latitude": c.lat, "longitude": c.lng })
}
