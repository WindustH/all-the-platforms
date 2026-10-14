//! Places as users give them: `LNG,LAT` points (in the `--coord` system,
//! longitude first) and POI ids, bare or inside an amap.com place link.

use atp_core::geo::{Coord, System};
use atp_core::{Error, Result};
use url::Url;

/// `LNG,LAT` in `system`, as Amap's GCJ-02 `lng,lat` (six decimals).
pub fn expect_location(arg: &str, system: System) -> Result<String> {
  Ok(Coord::expect(arg, system)?.to(System::Gcj02).lng_lat())
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
