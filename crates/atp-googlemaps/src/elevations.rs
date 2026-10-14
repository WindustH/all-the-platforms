//! Elevation API: metres above sea level at up to 512 points.

use atp_core::geo::Coord;
use atp_core::{Error, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs;

pub async fn elevation(api: &Api, points: &[String]) -> Result<Value> {
  if points.len() > 512 {
    return Err(Error::input("Google takes at most 512 points"));
  }
  let system = api.system();
  let coords: Vec<Coord> = points
    .iter()
    .map(|p| refs::expect_point(p, system))
    .collect::<Result<_>>()?;
  let locations: Vec<String> = coords.iter().map(Coord::lat_lng).collect();
  let v = api
    .service("/elevation/json", &[("locations", locations.join("|"))])
    .await?;
  let results: Vec<Value> = v
    .list("results")
    .iter()
    .map(|r| {
      json!({
        "location": refs::point(r.at("location")),
        "elevation_m": r.f64("elevation").map(|e| (e * 10.0).round() / 10.0),
        "resolution_m": r.f64("resolution").map(|e| (e * 10.0).round() / 10.0),
      })
    })
    .collect();
  Ok(Value::from(results))
}
