//! Route planning (`/ws/direction/v1/*`, durations in minutes) and the
//! distance matrix (`/ws/distance/v1/matrix`, durations in seconds). Ends
//! are `LNG,LAT` or place names, geocoded first.

use atp_core::geo::{Distance, Measure, Mode, Route, Routes, Step};
use atp_core::{Error, Price, Result, Value, ValueExt};

use crate::api::Api;
use crate::geocoding::{self, End};
use crate::parse;

/// `--strategy` names and Tencent's `policy` of driving and transit.
const DRIVING: &[(&str, &str)] = &[
  ("recommended", "LEAST_TIME"),
  ("avoid-jam", "LEAST_TIME,REAL_TRAFFIC"),
  ("least-fee", "LEAST_TIME,LEAST_FEE"),
  ("highway", "LEAST_TIME,HIGHWAY_FIRST"),
  ("no-highway", "LEAST_TIME,AVOID_HIGHWAY"),
  ("big-road", "LEAST_TIME,HIGHROAD_FIRST"),
];
const TRANSIT: &[(&str, &str)] = &[
  ("recommended", "RECOMMEND"),
  ("fastest", "LEAST_TIME"),
  ("fewest-transfers", "LEAST_TRANSFER"),
  ("least-walk", "LEAST_WALKING"),
  ("no-subway", "LEAST_TIME,NO_SUBWAY"),
  ("subway", "LEAST_TIME,SUBWAY_FIRST"),
];

fn policy(mode: Mode, name: Option<&str>) -> Result<String> {
  let Some(name) = name else {
    return Ok(String::new());
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
    .map(|(_, p)| (*p).to_owned())
    .ok_or_else(|| {
      Error::input(format!(
        "unknown strategy `{name}` (one of {})",
        names.join(", ")
      ))
    })
}

fn path(mode: Mode) -> &'static str {
  match mode {
    Mode::Driving => "driving",
    Mode::Walking => "walking",
    Mode::Cycling => "bicycling",
    Mode::Ebike => "ebicycling",
    Mode::Transit => "transit",
  }
}

pub async fn route(
  api: &Api,
  from: &str,
  to: &str,
  mode: Mode,
  city: Option<&str>,
  strategy: Option<&str>,
) -> Result<Routes> {
  let origin = geocoding::end(api, from, city).await?;
  let destination = geocoding::end(api, to, city).await?;
  let params = [
    ("from", origin.location.lat_lng()),
    ("to", destination.location.lat_lng()),
    ("policy", policy(mode, strategy)?),
    (
      "get_mp",
      if mode == Mode::Driving { "1" } else { "" }.to_owned(),
    ),
  ];
  let url = format!("/ws/direction/v1/{}/", path(mode));
  let v = api.get(&url, &params).await?;
  let routes = v
    .list("result.routes")
    .iter()
    .map(|r| match mode {
      Mode::Transit => transit(r),
      _ => drive(r),
    })
    .collect();
  Ok(Routes {
    mode,
    origin: origin.waypoint(),
    destination: destination.waypoint(),
    routes,
    extra: Default::default(),
  })
}

/// Minutes → seconds.
fn minutes(v: &Value, path: &str) -> Option<u64> {
  v.f64(path).map(|m| (m * 60.0).round() as u64)
}

/// A driving, walking or cycling route.
fn drive(r: &Value) -> Route {
  let steps = r
    .list("steps")
    .iter()
    .map(|s| Step {
      instruction: s.str("instruction").unwrap_or_default(),
      road: s.str("road_name"),
      distance_m: s.u64("distance"),
      duration_s: minutes(s, "duration"),
      ..Step::default()
    })
    .collect();
  let tags: Vec<String> = r
    .list("tags")
    .iter()
    .filter_map(|t| t.as_str().map(str::to_owned))
    .collect();
  let mut route = Route {
    distance_m: r.u64("distance"),
    duration_s: minutes(r, "duration"),
    label: (!tags.is_empty()).then(|| tags.join(", ")),
    tolls: r.f64("toll").map(Price::cny),
    steps,
    polyline: parse::polyline(r.at("polyline")),
    ..Route::default()
  };
  for (key, value) in [
    (
      "traffic_lights",
      r.u64("traffic_light_count").map(Value::from),
    ),
    ("taxi_cost", r.f64("taxi_fare.fare").map(Value::from)),
    ("restriction", r.u64("restriction.status").map(Value::from)),
  ] {
    if let Some(value) = value {
      route.extra.insert(key.into(), value);
    }
  }
  route
}

/// A transit plan: walks and rides as legs.
fn transit(r: &Value) -> Route {
  let mut fare = 0.0;
  let steps = r
    .list("steps")
    .iter()
    .map(|s| {
      let Some(line) = s.list("lines").first() else {
        let d = s.u64("distance");
        return leg(
          "walk",
          format!(
            "walk {}",
            d.map(atp_core::geo::fmt_distance).unwrap_or_default()
          ),
          s,
        );
      };
      // Buses and subways are priced in fen, rail in yuan; -1 is unknown.
      let price = line.f64("price").filter(|p| *p >= 0.0);
      let vehicle = line.str("vehicle").unwrap_or_default().to_lowercase();
      fare += price.map_or(0.0, |p| if vehicle == "rail" { p } else { p / 100.0 });
      let stops = line
        .u64("station_count")
        .map_or(String::new(), |n| format!(", {n} stops"));
      let text = format!(
        "{}: {} → {}{stops}",
        line.str("title").unwrap_or_default(),
        line.str("geton.title").unwrap_or_default(),
        line.str("getoff.title").unwrap_or_default(),
      );
      leg(&vehicle, text, line)
    })
    .collect();
  Route {
    distance_m: r.u64("distance"),
    duration_s: minutes(r, "duration"),
    fare: (fare > 0.0).then(|| Price::cny(fare)),
    steps,
    ..Route::default()
  }
}

fn leg(mode: &str, instruction: String, v: &Value) -> Step {
  Step {
    instruction,
    mode: Some(mode.to_owned()),
    distance_m: v.u64("distance"),
    duration_s: minutes(v, "duration"),
    ..Step::default()
  }
}

/// Distances from each place to each destination.
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
  let mode = match measure {
    Measure::Straight => "straight",
    Measure::Driving => "driving",
    Measure::Walking => "walking",
    Measure::Cycling => "bicycling",
    Measure::Ebike => "ebicycling",
  };
  let join = |ends: &[End]| {
    ends
      .iter()
      .map(|e| e.location.lat_lng())
      .collect::<Vec<_>>()
      .join(";")
  };
  let params = [
    ("mode", mode.to_owned()),
    ("from", join(&origins)),
    ("to", join(&destinations)),
  ];
  let v = api.get("/ws/distance/v1/matrix", &params).await?;
  let mut out = Vec::new();
  for (o, row) in origins.iter().zip(v.list("result.rows")) {
    for (d, cell) in destinations.iter().zip(row.list("elements")) {
      out.push(Distance {
        from: o.waypoint(),
        to: d.waypoint(),
        mode: measure,
        distance_m: cell.u64("distance"),
        duration_s: cell.u64("duration"),
      });
    }
  }
  Ok(out)
}
