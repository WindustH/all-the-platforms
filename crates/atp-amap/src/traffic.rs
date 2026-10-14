//! Live traffic (`/v3/traffic/status/*`) on a named road, in a rectangle,
//! or around a point.

use atp_core::{Error, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{districts, refs};

/// Where to look.
pub enum Area<'a> {
  Road { name: &'a str, city: &'a str },
  Rect(&'a str),
  Around { location: &'a str, radius: u32 },
}

pub async fn status(api: &Api, area: Area<'_>) -> Result<Value> {
  let (kind, params) = match area {
    Area::Road { name, city } => {
      let adcode = districts::adcode(api, city).await?;
      ("road", vec![("name", name.to_owned()), ("adcode", adcode)])
    }
    Area::Rect(rect) => ("rectangle", vec![("rectangle", rectangle(rect)?)]),
    Area::Around { location, radius } => {
      let params = vec![
        ("location", refs::expect_location(location)?),
        ("radius", radius.clamp(1, 5000).to_string()),
      ];
      ("circle", params)
    }
  };
  let mut params = params;
  params.push(("extensions", "all".to_owned()));
  let v = api
    .get(&format!("/v3/traffic/status/{kind}"), &params)
    .await?;
  Ok(summary(v.at("trafficinfo")))
}

/// `LNG,LAT;LNG,LAT`: the south-west and north-east corners.
fn rectangle(arg: &str) -> Result<String> {
  let corners: Vec<String> = arg
    .split([';', '|'])
    .map(refs::expect_location)
    .collect::<Result<_>>()?;
  if corners.len() != 2 {
    return Err(Error::input("--rect takes two corners: LNG,LAT;LNG,LAT"));
  }
  Ok(corners.join(";"))
}

/// Amap's road status codes.
fn level(code: Option<String>) -> &'static str {
  match code.as_deref() {
    Some("1") => "smooth",
    Some("2") => "slow",
    Some("3") => "congested",
    Some("4") => "severe",
    _ => "unknown",
  }
}

fn summary(t: &Value) -> Value {
  let e = t.at("evaluation");
  let roads: Vec<Value> = t
    .list("roads")
    .iter()
    .map(|r| {
      json!({
        "name": r.str("name"),
        "status": level(r.str("status")),
        "direction": r.str("direction"),
        "speed_kmh": r.u64("speed"),
      })
    })
    .collect();
  json!({
    "description": t.str("description"),
    "status": level(e.str("status")),
    "smooth": e.str("expedite"),
    "slow": e.str("congested"),
    "congested": e.str("blocked"),
    "unknown": e.str("unknown"),
    "roads": roads,
  })
}
