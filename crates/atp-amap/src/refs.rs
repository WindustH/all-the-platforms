//! Places as users give them: `lng,lat` coordinates (Amap's GCJ-02, longitude
//! first) and POI ids, bare or inside an amap.com place link.

use atp_core::{Error, Result};
use url::Url;

/// `lng,lat` checked and trimmed to six decimals, Amap's precision.
pub fn location(arg: &str) -> Option<String> {
  let (lng, lat) = arg.trim().split_once(',')?;
  let lng: f64 = lng.trim().parse().ok()?;
  let lat: f64 = lat.trim().parse().ok()?;
  ((-180.0..=180.0).contains(&lng) && (-90.0..=90.0).contains(&lat))
    .then(|| format!("{},{}", decimals(lng), decimals(lat)))
}

pub fn expect_location(arg: &str) -> Result<String> {
  location(arg).ok_or_else(|| {
    Error::input(format!(
      "not a location: `{arg}` (expected LNG,LAT, e.g. 116.397428,39.90923)"
    ))
  })
}

fn decimals(x: f64) -> String {
  let s = format!("{x:.6}");
  s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A POI id from `B000A7BD6C`, `https://www.amap.com/place/B000A7BD6C` or a
/// link carrying `poiid=` / `id=`.
pub fn poi_id(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let id = match Url::parse(arg) {
    Ok(url) => from_link(&url),
    Err(_) => Some(arg.to_owned()),
  };
  id.filter(|id| (6..=24).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric()))
    .ok_or_else(|| {
      Error::input(format!(
        "not an Amap place: `{arg}` (expected a POI id like B000A7BD6C)"
      ))
    })
}

fn from_link(url: &Url) -> Option<String> {
  let query = |name: &str| {
    url
      .query_pairs()
      .find(|(k, _)| k == name)
      .map(|(_, v)| v.into_owned())
  };
  if let Some(id) = query("poiid").or_else(|| query("id")) {
    return Some(id);
  }
  let mut segments = url.path_segments()?;
  segments.find(|s| *s == "place")?;
  segments.next().map(str::to_owned)
}

pub fn place_url(id: &str) -> String {
  format!("https://www.amap.com/place/{id}")
}
