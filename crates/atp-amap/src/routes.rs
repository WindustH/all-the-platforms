//! Route planning 2.0 (`/v5/direction/*`) and batch distances
//! (`/v3/distance`). Ends are `lng,lat` or place names, geocoded first.

use atp_core::text::fmt_duration;
use atp_core::{Error, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::geocoding::{self, Place};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Mode {
  Driving,
  Walking,
  Cycling,
  Ebike,
  Transit,
}

impl Mode {
  fn name(self) -> &'static str {
    match self {
      Mode::Driving => "driving",
      Mode::Walking => "walking",
      Mode::Cycling => "cycling",
      Mode::Ebike => "ebike",
      Mode::Transit => "transit",
    }
  }

  fn path(self) -> &'static str {
    match self {
      Mode::Driving => "driving",
      Mode::Walking => "walking",
      Mode::Cycling => "bicycling",
      Mode::Ebike => "electrobike",
      Mode::Transit => "transit/integrated",
    }
  }
}

/// `--strategy` names of driving and transit routes and their codes.
const DRIVING: &[(&str, &str)] = &[
  ("recommended", "32"),
  ("avoid-jam", "33"),
  ("highway", "34"),
  ("no-highway", "35"),
  ("least-fee", "36"),
  ("big-road", "37"),
  ("fastest", "38"),
  ("no-toll", "1"),
];
const TRANSIT: &[(&str, &str)] = &[
  ("recommended", "0"),
  ("cheapest", "1"),
  ("fewest-transfers", "2"),
  ("least-walk", "3"),
  ("comfortable", "4"),
  ("no-subway", "5"),
  ("subway", "7"),
  ("fastest", "8"),
];

fn strategy(mode: Mode, name: Option<&str>) -> Result<Option<String>> {
  let Some(name) = name else {
    return Ok(None);
  };
  let table = match mode {
    Mode::Driving => DRIVING,
    Mode::Transit => TRANSIT,
    _ => {
      return Err(Error::input(
        "--strategy applies to driving and transit routes",
      ));
    }
  };
  let names: Vec<&str> = table.iter().map(|(n, _)| *n).collect();
  table
    .iter()
    .find(|(n, _)| *n == name)
    .map(|(_, code)| Some((*code).to_owned()))
    .ok_or_else(|| {
      Error::input(format!(
        "unknown strategy `{name}` (one of {})",
        names.join(", ")
      ))
    })
}

pub async fn route(
  api: &Api,
  from: &str,
  to: &str,
  mode: Mode,
  city: Option<&str>,
  strategy_name: Option<&str>,
) -> Result<Value> {
  let origin = geocoding::place(api, from, city).await?;
  let destination = geocoding::place(api, to, city).await?;
  let mut params = vec![
    ("origin", origin.location.clone()),
    ("destination", destination.location.clone()),
    ("show_fields", "cost".to_owned()),
  ];
  if let Some(code) = strategy(mode, strategy_name)? {
    params.push(("strategy", code));
  }
  if mode == Mode::Transit {
    params.push(("city1", geocoding::citycode(api, &origin).await?));
    params.push(("city2", geocoding::citycode(api, &destination).await?));
  }
  let v = api
    .get(&format!("/v5/direction/{}", mode.path()), &params)
    .await?;
  let r = v.at("route");
  let routes: Vec<Value> = match mode {
    Mode::Transit => r.list("transits").iter().map(transit).collect(),
    _ => r.list("paths").iter().map(path).collect(),
  };
  Ok(json!({
    "mode": mode.name(),
    "origin": end(&origin),
    "destination": end(&destination),
    "taxi_cost": r.str("taxi_cost"),
    "routes": routes,
  }))
}

fn end(place: &Place) -> Value {
  json!({ "name": place.name, "location": place.location })
}

/// Travel time as text and seconds; cycling paths carry it outside `cost`.
fn duration(v: &Value) -> (Option<String>, Option<u64>) {
  let secs = v.u64("cost.duration").or_else(|| v.u64("duration"));
  (secs.map(|s| fmt_duration(s as f64)), secs)
}

/// One driving / walking / cycling path with its turn-by-turn steps.
fn path(p: &Value) -> Value {
  let (time, secs) = duration(p);
  let steps: Vec<Value> = p
    .list("steps")
    .iter()
    .map(|s| {
      json!({
        "instruction": s.str("instruction"),
        "road": s.str("road_name"),
        "distance_m": s.u64("step_distance"),
      })
    })
    .collect();
  json!({
    "distance_m": p.u64("distance"),
    "duration": time,
    "duration_s": secs,
    "tolls": p.str("cost.tolls"),
    "toll_distance_m": p.u64("cost.toll_distance"),
    "traffic_lights": p.u64("cost.traffic_lights"),
    "restricted": p.str("restriction").as_deref() == Some("1"),
    "steps": steps,
  })
}

/// One public-transport plan, its legs as readable lines.
fn transit(t: &Value) -> Value {
  let (time, secs) = duration(t);
  let legs: Vec<String> = t.list("segments").iter().flat_map(legs).collect();
  json!({
    "duration": time,
    "duration_s": secs,
    "distance_m": t.u64("distance"),
    "walking_m": t.u64("walking_distance"),
    "fee": t.str("cost.transit_fee"),
    "night": t.str("nightflag").as_deref() == Some("1"),
    "legs": legs,
  })
}

fn legs(s: &Value) -> Vec<String> {
  let mut out = Vec::new();
  if let Some(d) = s.u64("walking.distance").filter(|d| *d > 0) {
    out.push(format!("walk {d} m"));
  }
  if let Some(line) = s.list("bus.buslines").first() {
    let stops = line
      .u64("via_num")
      .map_or(String::new(), |n| format!(", {} stops", n + 1));
    out.push(format!(
      "{}: {} → {}{stops}",
      line.str("name").unwrap_or_default(),
      line.str("departure_stop.name").unwrap_or_default(),
      line.str("arrival_stop.name").unwrap_or_default(),
    ));
  }
  if let Some(name) = s.str("railway.name") {
    out.push(format!(
      "{name}: {} → {}",
      s.str("railway.departure_stop.name").unwrap_or_default(),
      s.str("railway.arrival_stop.name").unwrap_or_default(),
    ));
  }
  if let Some(d) = s.u64("taxi.distance").filter(|d| *d > 0) {
    out.push(format!("taxi {d} m"));
  }
  out
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Measure {
  Straight,
  Driving,
  Walking,
}

/// Distances from up to 100 places to one destination.
pub async fn distance(api: &Api, from: &[String], to: &str, measure: Measure) -> Result<Value> {
  let mut origins = Vec::new();
  for arg in from {
    origins.push(geocoding::place(api, arg, None).await?);
  }
  let destination = geocoding::place(api, to, None).await?;
  let kind = match measure {
    Measure::Straight => "0",
    Measure::Driving => "1",
    Measure::Walking => "3",
  };
  let locations: Vec<String> = origins.iter().map(|p| p.location.clone()).collect();
  let params = [
    ("origins", locations.join("|")),
    ("destination", destination.location.clone()),
    ("type", kind.to_owned()),
  ];
  let v = api.get("/v3/distance", &params).await?;
  let results: Vec<Value> = v
    .list("results")
    .iter()
    .map(|r| {
      let index = r.u64("origin_id").unwrap_or(1).saturating_sub(1) as usize;
      json!({
        "from": origins.get(index).map(end),
        "distance_m": r.u64("distance"),
        "duration_s": r.u64("duration"),
      })
    })
    .collect();
  Ok(json!({ "to": end(&destination), "results": results }))
}
