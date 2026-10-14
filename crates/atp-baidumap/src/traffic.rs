//! Real-time traffic v1 (`/traffic/v1/*`) on a named road, in a rectangle
//! (diagonal at most 2 km) or around a point (radius at most 1 km).

use atp_core::geo::Coord;
use atp_core::{Error, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs;

/// Where to look.
pub enum Area<'a> {
  Road { name: &'a str, city: &'a str },
  Rect(&'a str),
  Around { location: &'a str, radius: u32 },
}

pub async fn status(api: &Api, area: Area<'_>) -> Result<Value> {
  let wire = api.wire();
  let (kind, mut params) = match area {
    Area::Road { name, city } => (
      "road",
      vec![("road_name", name.to_owned()), ("city", city.to_owned())],
    ),
    Area::Rect(rect) => ("bound", vec![("bounds", rectangle(api, rect)?)]),
    Area::Around { location, radius } => {
      let center = refs::expect_point(location, api.system(), wire)?;
      let params = vec![
        ("center", center.lat_lng()),
        ("radius", radius.clamp(1, 1000).to_string()),
      ];
      ("around", params)
    }
  };
  params.push(("coord_type_input", refs::short(wire).to_owned()));
  let v = api.get(&format!("/traffic/v1/{kind}"), &params).await?;
  Ok(summary(&v))
}

/// `LNG,LAT;LNG,LAT` (south-west, north-east) as Baidu's `lat,lng;lat,lng`.
fn rectangle(api: &Api, arg: &str) -> Result<String> {
  let corners: Vec<Coord> = arg
    .split([';', '|'])
    .map(|c| refs::expect_point(c, api.system(), api.wire()))
    .collect::<Result<_>>()?;
  if corners.len() != 2 {
    return Err(Error::input("--rect takes two corners: LNG,LAT;LNG,LAT"));
  }
  Ok(format!("{};{}", corners[0].lat_lng(), corners[1].lat_lng()))
}

/// Baidu's road status codes.
fn level(code: Option<i64>) -> &'static str {
  match code {
    Some(1) => "smooth",
    Some(2) => "slow",
    Some(3) => "congested",
    Some(4) => "severe",
    _ => "unknown",
  }
}

fn summary(v: &Value) -> Value {
  let roads: Vec<Value> = v
    .list("road_traffic")
    .iter()
    .map(|r| {
      let sections: Vec<Value> = r
        .list("congestion_sections")
        .iter()
        .map(|s| {
          json!({
            "description": s.str("section_desc"),
            "status": level(s.i64("status")),
            "speed_kmh": s.f64("speed"),
            "length_m": s.u64("congestion_distance"),
            "trend": s.str("congestion_trend"),
          })
        })
        .collect();
      json!({ "name": r.str("road_name"), "sections": sections })
    })
    .collect();
  json!({
    "description": v.str("description"),
    "status": level(v.i64("evaluation.status")),
    "status_text": v.str("evaluation.status_desc"),
    "roads": roads,
  })
}
