//! Route planning 2.0 (`/v5/direction/*`) and batch distances
//! (`/v3/distance`). Ends are `LNG,LAT` or place names, geocoded first.

use atp_core::geo::{self, Distance, Mode, Polyline, Route, Routes, Step, System};
use atp_core::{Error, Price, Result, Value, ValueExt};

use crate::api::Api;
use crate::geocoding::{self, End};

fn path(mode: Mode) -> &'static str {
  match mode {
    Mode::Driving => "driving",
    Mode::Walking => "walking",
    Mode::Cycling => "bicycling",
    Mode::Ebike => "electrobike",
    Mode::Transit => "transit/integrated",
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
) -> Result<Routes> {
  let origin = geocoding::end(api, from, city).await?;
  let destination = geocoding::end(api, to, city).await?;
  let mut params = vec![
    ("origin", origin.lng_lat()),
    ("destination", destination.lng_lat()),
    ("show_fields", "cost,polyline".to_owned()),
  ];
  if let Some(code) = strategy(mode, strategy_name)? {
    params.push(("strategy", code));
  }
  if mode == Mode::Transit {
    params.push(("city1", geocoding::citycode(api, &origin).await?));
    params.push(("city2", geocoding::citycode(api, &destination).await?));
  }
  let v = api
    .get(&format!("/v5/direction/{}", path(mode)), &params)
    .await?;
  let r = v.at("route");
  let routes: Vec<Route> = match mode {
    Mode::Transit => r.list("transits").iter().map(transit).collect(),
    _ => r.list("paths").iter().map(drive).collect(),
  };
  let mut plan = Routes {
    mode,
    origin: origin.waypoint(),
    destination: destination.waypoint(),
    routes,
    extra: Default::default(),
  };
  if let Some(cost) = r.f64("taxi_cost") {
    plan.extra.insert("taxi_cost".into(), cost.into());
  }
  Ok(plan)
}

/// Travel time in seconds; cycling paths carry it outside `cost`.
fn duration(v: &Value) -> Option<u64> {
  v.u64("cost.duration").or_else(|| v.u64("duration"))
}

/// A step's shape: a `lng,lat;…` string, or one inside `polyline`.
fn shape(v: &Value) -> Option<String> {
  v.str("polyline").or_else(|| v.str("polyline.polyline"))
}

/// One driving / walking / cycling path with its turn-by-turn steps.
fn drive(p: &Value) -> Route {
  let mut points = Vec::new();
  let steps: Vec<Step> = p
    .list("steps")
    .iter()
    .map(|s| {
      if let Some(line) = shape(s) {
        points.extend(Polyline::from_path(System::Gcj02, &line).points());
      }
      Step {
        instruction: s.str("instruction").unwrap_or_default(),
        road: s.str("road_name"),
        distance_m: s.u64("step_distance"),
        duration_s: s.u64("cost.duration"),
        ..Step::default()
      }
    })
    .collect();
  let mut route = Route {
    distance_m: p.u64("distance"),
    duration_s: duration(p),
    tolls: p.f64("cost.tolls").map(Price::cny),
    steps,
    polyline: (!points.is_empty()).then(|| Polyline::new(System::Gcj02, &points)),
    ..Route::default()
  };
  for (key, value) in [
    (
      "toll_distance_m",
      p.u64("cost.toll_distance").map(Value::from),
    ),
    (
      "traffic_lights",
      p.u64("cost.traffic_lights").map(Value::from),
    ),
    (
      "restricted",
      Some((p.str("restriction").as_deref() == Some("1")).into()),
    ),
  ] {
    if let Some(value) = value {
      route.extra.insert(key.into(), value);
    }
  }
  route
}

/// One public-transport plan, its legs as steps.
fn transit(t: &Value) -> Route {
  let mut route = Route {
    distance_m: t.u64("distance"),
    duration_s: duration(t),
    fare: t.f64("cost.transit_fee").map(Price::cny),
    steps: t.list("segments").iter().flat_map(legs).collect(),
    ..Route::default()
  };
  if let Some(walk) = t.u64("walking_distance") {
    route.extra.insert("walking_m".into(), walk.into());
  }
  let night = t.str("nightflag").as_deref() == Some("1");
  route.extra.insert("night".into(), night.into());
  route
}

fn legs(s: &Value) -> Vec<Step> {
  let mut out = Vec::new();
  if let Some(d) = s.u64("walking.distance").filter(|d| *d > 0) {
    out.push(leg(
      "walk",
      format!("walk {}", geo::fmt_distance(d)),
      Some(d),
    ));
  }
  if let Some(line) = s.list("bus.buslines").first() {
    let name = line.str("name").unwrap_or_default();
    let stops = line
      .u64("via_num")
      .map_or(String::new(), |n| format!(", {} stops", n + 1));
    let text = format!(
      "{name}: {} → {}{stops}",
      line.str("departure_stop.name").unwrap_or_default(),
      line.str("arrival_stop.name").unwrap_or_default(),
    );
    let kind = line.str("type").unwrap_or_default();
    let mode = if kind.contains("地铁") {
      "subway"
    } else {
      "bus"
    };
    let mut step = leg(mode, text, line.u64("distance"));
    step.duration_s = line.u64("cost.duration");
    out.push(step);
  }
  if let Some(name) = s.str("railway.name") {
    let text = format!(
      "{name}: {} → {}",
      s.str("railway.departure_stop.name").unwrap_or_default(),
      s.str("railway.arrival_stop.name").unwrap_or_default(),
    );
    out.push(leg("rail", text, s.u64("railway.distance")));
  }
  if let Some(d) = s.u64("taxi.distance").filter(|d| *d > 0) {
    out.push(leg(
      "taxi",
      format!("taxi {}", geo::fmt_distance(d)),
      Some(d),
    ));
  }
  out
}

fn leg(mode: &str, instruction: String, distance_m: Option<u64>) -> Step {
  Step {
    instruction,
    mode: Some(mode.to_owned()),
    distance_m,
    ..Step::default()
  }
}

/// How `distance` measures (Amap offers no cycling distances).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Measure {
  Straight,
  Driving,
  Walking,
}

/// Distances from up to 100 places to one destination.
pub async fn distance(
  api: &Api,
  from: &[String],
  to: &str,
  measure: Measure,
) -> Result<Vec<Distance>> {
  let mut origins: Vec<End> = Vec::new();
  for arg in from {
    origins.push(geocoding::end(api, arg, None).await?);
  }
  let destination = geocoding::end(api, to, None).await?;
  let (kind, mode) = match measure {
    Measure::Straight => ("0", geo::Measure::Straight),
    Measure::Driving => ("1", geo::Measure::Driving),
    Measure::Walking => ("3", geo::Measure::Walking),
  };
  let locations: Vec<String> = origins.iter().map(End::lng_lat).collect();
  let params = [
    ("origins", locations.join("|")),
    ("destination", destination.lng_lat()),
    ("type", kind.to_owned()),
  ];
  let v = api.get("/v3/distance", &params).await?;
  let results = v.list("results").iter().filter_map(|r| {
    let index = r.u64("origin_id").unwrap_or(1).saturating_sub(1) as usize;
    Some(Distance {
      from: origins.get(index)?.waypoint(),
      to: destination.waypoint(),
      mode,
      distance_m: r.u64("distance"),
      duration_s: r.u64("duration"),
    })
  });
  Ok(results.collect())
}
