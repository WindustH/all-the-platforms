//! What users give: `LNG,LAT` points (in the `--coord` system) and place
//! ids, bare or inside a map.qq.com link. Tencent takes and returns
//! GCJ-02 `lat,lng`.

use atp_core::geo::{Coord, System};
use atp_core::{Error, Result, Value, ValueExt};

/// `LNG,LAT` in `system`, in Tencent's GCJ-02.
pub fn point(arg: &str, system: System) -> Option<Coord> {
  Coord::parse(arg, system).map(|c| c.to(System::Gcj02))
}

pub fn expect_point(arg: &str, system: System) -> Result<Coord> {
  Ok(Coord::expect(arg, system)?.to(System::Gcj02))
}

/// A `{lat, lng}` object.
pub fn located(v: &Value) -> Option<Coord> {
  Some(Coord::new(v.f64("lng")?, v.f64("lat")?, System::Gcj02))
}

/// A place id: digits as Tencent writes them, bare or after `uid=` /
/// `id=` in a link.
pub fn place_id(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let id = ["uid=", "id="]
    .iter()
    .find_map(|k| arg.split_once(k).map(|(_, rest)| rest))
    .map_or(arg, |rest| rest.split(['&', '#']).next().unwrap_or(rest));
  if !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
    return Ok(id.to_owned());
  }
  Err(Error::input(format!(
    "not a Tencent place: `{arg}` (expected an id like 3629720141162880123)"
  )))
}

/// The link `read` takes back.
pub fn place_url(id: &str) -> String {
  format!("https://map.qq.com/?type=poi&uid={id}")
}
