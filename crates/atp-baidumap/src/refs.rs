//! What users give: `LNG,LAT` points (in the `--coord` system) and place
//! uids, bare or inside a map.baidu.com link; and how Baidu names the
//! coordinate systems in its parameters.

use atp_core::geo::{Coord, System};
use atp_core::{Error, Result, Value, ValueExt};
use url::Url;

/// `LNG,LAT` in the user's system, converted to the wire system.
pub fn point(arg: &str, user: System, wire: System) -> Option<Coord> {
  Coord::parse(arg, user).map(|c| c.to(wire))
}

pub fn expect_point(arg: &str, user: System, wire: System) -> Result<Coord> {
  Ok(Coord::expect(arg, user)?.to(wire))
}

/// `coordtype` / `ret_coordtype` of geocoding, place search and static
/// maps: `bd09ll`, `gcj02ll`, `wgs84ll`.
pub fn ll(system: System) -> &'static str {
  match system {
    System::Bd09 => "bd09ll",
    System::Gcj02 => "gcj02ll",
    System::Wgs84 => "wgs84ll",
  }
}

/// `coord_type` of directions, route matrices, traffic and weather:
/// `bd09ll`, `gcj02`, `wgs84`.
pub fn short(system: System) -> &'static str {
  match system {
    System::Bd09 => "bd09ll",
    System::Gcj02 => "gcj02",
    System::Wgs84 => "wgs84",
  }
}

/// `coord_type` of place search and suggestions: 1 WGS-84, 2 GCJ-02,
/// 3 BD-09.
pub fn number(system: System) -> &'static str {
  match system {
    System::Wgs84 => "1",
    System::Gcj02 => "2",
    System::Bd09 => "3",
  }
}

/// A `{lng, lat}` object, or `{x, y}` (reverse-geocoded places, IPs).
pub fn located(v: &Value, system: System) -> Option<Coord> {
  let lng = v.f64("lng").or_else(|| v.f64("x"))?;
  let lat = v.f64("lat").or_else(|| v.f64("y"))?;
  Some(Coord::new(lng, lat, system))
}

/// A place uid from `435d7aea036e54355abbbcc8` or a link carrying `uid=`
/// or `poiShareUid=`.
pub fn uid(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let id = match Url::parse(arg) {
    Ok(url) => url
      .query_pairs()
      .find_map(|(k, v)| matches!(k.as_ref(), "uid" | "poiShareUid").then(|| v.into_owned())),
    Err(_) => Some(arg.to_owned()),
  };
  id.filter(|id| (10..=40).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric()))
    .ok_or_else(|| {
      Error::input(format!(
        "not a Baidu place: `{arg}` (expected a uid like 435d7aea036e54355abbbcc8)"
      ))
    })
}

/// The link `read` takes back.
pub fn place_url(uid: &str) -> String {
  format!("https://map.baidu.com/?shareurl=1&poiShareUid={uid}")
}
