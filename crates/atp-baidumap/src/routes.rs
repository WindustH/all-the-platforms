//! Direction v2 (`/direction/v2/*`) and route matrices
//! (`/routematrix/v2/*`). Ends are `LNG,LAT` or place names, geocoded
//! first.

use atp_core::geo::{self, Distance, Measure, Mode, Polyline, Route, Routes, Step, System};
use atp_core::{Error, Price, Result, Value, ValueExt};

use crate::api::Api;
use crate::geocoding::{self, End};
use crate::refs;

/// `--strategy` names and Baidu's `tactics` (driving) or
/// `tactics_incity` (transit).
const DRIVING: &[(&str, &str)] = &[
  ("recommended", "0"),
  ("shortest", "2"),
  ("no-highway", "3"),
  ("highway", "4"),
  ("avoid-jam", "5"),
  ("least-fee", "6"),
  ("fastest", "13"),
];
const TRANSIT: &[(&str, &str)] = &[
  ("recommended", "0"),
  ("fewest-transfers", "1"),
  ("least-walk", "2"),
  ("no-subway", "3"),
  ("fastest", "4"),
  ("subway", "5"),
];

fn strategy(mode: Mode, name: Option<&str>) -> Result<Option<(&'static str, String)>> {
  let Some(name) = name else {
    return Ok(None);
  };
  let (key, table) = match mode {
    Mode::Driving => ("tactics", DRIVING),
    Mode::Transit => ("tactics_incity", TRANSIT),
    _ => {
      return Err(Error::input(
        "--strategy applies to driving and transit routes",
      ));
    }
  };
  let names: Vec<&str> = table.iter().map(|(n, _)| *n).collect();
  let (_, code) = table.iter().find(|(n, _)| *n == name).ok_or_else(|| {
    Error::input(format!(
      "unknown strategy `{name}` (one of {})",
      names.join(", ")
    ))
  })?;
  Ok(Some((key, (*code).to_owned())))
}

/// Coordinate systems, alternatives, the riding type and `--strategy` of
/// a directions request.
fn options(
  mode: Mode,
  wire: System,
  strategy_name: Option<&str>,
) -> Result<Vec<(&'static str, String)>> {
  let returned = if wire == System::Bd09 { "" } else { "gcj02" };
  let mut params = vec![
    ("coord_type", refs::short(wire).to_owned()),
    ("ret_coordtype", returned.to_owned()),
  ];
  if mode == Mode::Driving {
    params.push(("alternatives", "1".into()));
  }
  if mode == Mode::Ebike {
    params.push(("riding_type", "1".into()));
  }
  params.extend(strategy(mode, strategy_name)?);
  Ok(params)
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
  let wire = api.wire();
  let path = match mode {
    Mode::Driving => "driving",
    Mode::Walking => "walking",
    Mode::Cycling | Mode::Ebike => "riding",
    Mode::Transit => "transit",
  };
  let mut params = vec![
    ("origin", origin.location.lat_lng()),
    ("destination", destination.location.lat_lng()),
  ];
  params.extend(options(mode, wire, strategy_name)?);
  let v = api.get(&format!("/direction/v2/{path}"), &params).await?;
  let r = v.at("result");
  let routes = r
    .list("routes")
    .iter()
    .map(|route| match mode {
      Mode::Transit => transit(route, wire),
      _ => drive(route, wire),
    })
    .collect();
  let mut plan = Routes {
    mode,
    origin: origin.waypoint(),
    destination: destination.waypoint(),
    routes,
    extra: Default::default(),
  };
  if let Some(taxi) = r.f64("taxi.detail.0.total_price") {
    plan.extra.insert("taxi_cost".into(), taxi.into());
  }
  Ok(plan)
}

/// A driving, riding or walking route.
fn drive(r: &Value, wire: System) -> Route {
  let mut points = Vec::new();
  let steps: Vec<Step> = r
    .list("steps")
    .iter()
    .map(|s| {
      if let Some(path) = s.str("path") {
        points.extend(Polyline::from_path(wire, &path).points());
      }
      let road = s.first_str(&["road_name", "name"]);
      let distance = s.u64("distance");
      let instruction = s
        .first_str(&["instruction", "instructions"])
        .map(|t| atp_core::text::html_to_text(&t))
        .unwrap_or_else(|| along(road.as_deref(), distance));
      Step {
        instruction,
        road,
        distance_m: distance,
        duration_s: s.u64("duration"),
        ..Step::default()
      }
    })
    .collect();
  let mut route = Route {
    distance_m: r.u64("distance"),
    duration_s: r.u64("duration"),
    label: r.str("tag"),
    tolls: r.f64("toll").map(Price::cny),
    steps,
    polyline: (!points.is_empty()).then(|| Polyline::new(wire, &points)),
    ..Route::default()
  };
  for (key, value) in [
    ("toll_distance_m", r.u64("toll_distance")),
    ("traffic_lights", r.u64("traffic_light")),
    ("taxi_cost", r.u64("taxi_fee")),
  ] {
    if let Some(value) = value {
      route.extra.insert(key.into(), value.into());
    }
  }
  route
}

/// `京藏高速 12.3 km` for driving steps, which carry no instruction.
fn along(road: Option<&str>, distance: Option<u64>) -> String {
  let road = road.unwrap_or("无名路");
  match distance {
    Some(d) => format!("{road} {}", geo::fmt_distance(d)),
    None => road.to_owned(),
  }
}

/// A transit plan: each step's first scheme as a leg.
fn transit(r: &Value, wire: System) -> Route {
  let mut points = Vec::new();
  let steps: Vec<Step> = r
    .list("steps")
    .iter()
    .filter_map(|s| match s {
      Value::Array(schemes) => schemes.first(),
      other => Some(other),
    })
    .map(|s| {
      if let Some(path) = s.str("path") {
        points.extend(Polyline::from_path(wire, &path).points());
      }
      leg(s)
    })
    .collect();
  Route {
    distance_m: r.u64("distance"),
    duration_s: r.u64("duration"),
    fare: r.f64("price").filter(|p| *p > 0.0).map(Price::cny),
    steps,
    polyline: (!points.is_empty()).then(|| Polyline::new(wire, &points)),
    ..Route::default()
  }
}

fn leg(s: &Value) -> Step {
  let v = s.at("vehicle_info");
  let d = v.at("detail");
  let mode = match (v.i64("type"), d.i64("type")) {
    (Some(1), _) => "rail",
    (Some(2), _) => "plane",
    (Some(3), Some(1)) => "subway",
    (Some(3 | 6), _) => "bus",
    (Some(4), _) => "drive",
    _ => "walk",
  };
  let instruction = match d.str("name") {
    Some(name) => {
      let stops = d
        .u64("stop_num")
        .map_or(String::new(), |n| format!(", {n} stops"));
      let from = d
        .first_str(&["on_station", "departure_station"])
        .unwrap_or_default();
      let to = d
        .first_str(&["off_station", "arrive_station"])
        .unwrap_or_default();
      format!("{name}: {from} → {to}{stops}")
    }
    None => s
      .str("instructions")
      .map(|t| atp_core::text::html_to_text(&t))
      .unwrap_or_default(),
  };
  Step {
    instruction,
    mode: Some(mode.into()),
    distance_m: s.u64("distance"),
    duration_s: s.u64("duration"),
    ..Step::default()
  }
}

/// Distances from each place to each destination (route matrix, or
/// straight lines).
pub async fn distance(
  api: &Api,
  from: &[String],
  to: &[String],
  measure: Measure,
) -> Result<Vec<Distance>> {
  let mut origins = Vec::new();
  for arg in from {
    origins.push(geocoding::end(api, arg, None).await?);
  }
  let mut destinations = Vec::new();
  for arg in to {
    destinations.push(geocoding::end(api, arg, None).await?);
  }
  let pairs = || {
    origins
      .iter()
      .flat_map(|o| destinations.iter().map(move |d| (o, d)))
  };
  if measure == Measure::Straight {
    let line = |(o, d): (&End, &End)| straight(o, d);
    return Ok(pairs().map(line).collect());
  }
  let (path, riding) = match measure {
    Measure::Walking => ("walking", ""),
    Measure::Cycling => ("riding", "0"),
    Measure::Ebike => ("riding", "1"),
    _ => ("driving", ""),
  };
  let join = |ends: &[End]| {
    ends
      .iter()
      .map(|e| e.location.lat_lng())
      .collect::<Vec<_>>()
      .join("|")
  };
  let params = [
    ("origins", join(&origins)),
    ("destinations", join(&destinations)),
    ("coord_type", refs::short(api.wire()).to_owned()),
    ("riding_type", riding.to_owned()),
  ];
  let v = api.get(&format!("/routematrix/v2/{path}"), &params).await?;
  let cells = v.list("result");
  let list = pairs().zip(cells).map(|((o, d), c)| Distance {
    from: o.waypoint(),
    to: d.waypoint(),
    mode: measure,
    distance_m: c
      .f64("distance.value")
      .filter(|m| *m > 0.0)
      .map(|m| m.round() as u64),
    duration_s: c
      .f64("duration.value")
      .filter(|s| *s > 0.0)
      .map(|s| s.round() as u64),
  });
  Ok(list.collect())
}

fn straight(o: &End, d: &End) -> Distance {
  Distance {
    from: o.waypoint(),
    to: d.waypoint(),
    mode: Measure::Straight,
    distance_m: Some(o.location.distance_m(&d.location).round() as u64),
    duration_s: None,
  }
}
