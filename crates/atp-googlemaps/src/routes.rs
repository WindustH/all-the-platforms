//! Routes API: `computeRoutes` and `computeRouteMatrix`. Requests stay in
//! the Essentials SKU unless a strategy needs live traffic (Pro) or the
//! mode is `ebike` (two-wheeler, Enterprise); tolls are not asked for.

use atp_core::geo::{Distance, Measure, Mode, Polyline, Route, Routes, Step};
use atp_core::{Error, Result, Value, ValueExt, json};

use crate::api::{Api, ROUTES};
use crate::geocoding::{self, End};
use crate::{parse, refs};

const ROUTE_FIELDS: &str = "routes.distanceMeters,routes.duration,routes.description,\
   routes.routeLabels,routes.polyline.encodedPolyline,routes.travelAdvisory.transitFare,\
   routes.legs.startLocation,routes.legs.endLocation,routes.legs.steps.distanceMeters,\
   routes.legs.steps.staticDuration,routes.legs.steps.navigationInstruction,\
   routes.legs.steps.travelMode,routes.legs.steps.transitDetails";
const MATRIX_FIELDS: &str = "originIndex,destinationIndex,status,condition,distanceMeters,duration";

fn travel_mode(mode: Mode) -> &'static str {
  match mode {
    Mode::Driving => "DRIVE",
    Mode::Walking => "WALK",
    Mode::Cycling => "BICYCLE",
    Mode::Ebike => "TWO_WHEELER",
    Mode::Transit => "TRANSIT",
  }
}

/// `--strategy` names of driving (and two-wheeler) routes.
const DRIVING: &[&str] = &[
  "recommended",
  "avoid-jam",
  "fastest",
  "no-toll",
  "least-fee",
  "no-highway",
  "no-ferry",
];
/// `--strategy` names of transit routes.
const TRANSIT: &[&str] = &[
  "recommended",
  "least-walk",
  "fewest-transfers",
  "subway",
  "no-subway",
  "bus",
  "rail",
];

/// Apply `--strategy` to a computeRoutes body.
fn strategy(body: &mut Value, mode: Mode, name: &str) -> Result<()> {
  let table = match mode {
    Mode::Driving | Mode::Ebike => DRIVING,
    Mode::Transit => TRANSIT,
    _ => {
      return Err(Error::input(
        "--strategy applies to driving, ebike and transit routes",
      ));
    }
  };
  if !table.contains(&name) {
    return Err(Error::input(format!(
      "unknown strategy `{name}` (one of {})",
      table.join(", ")
    )));
  }
  let (key, value) = match name {
    "avoid-jam" => ("routingPreference", json!("TRAFFIC_AWARE")),
    "fastest" => ("routingPreference", json!("TRAFFIC_AWARE_OPTIMAL")),
    "no-toll" | "least-fee" => ("routeModifiers", json!({ "avoidTolls": true })),
    "no-highway" => ("routeModifiers", json!({ "avoidHighways": true })),
    "no-ferry" => ("routeModifiers", json!({ "avoidFerries": true })),
    "least-walk" => (
      "transitPreferences",
      json!({ "routingPreference": "LESS_WALKING" }),
    ),
    "fewest-transfers" => (
      "transitPreferences",
      json!({ "routingPreference": "FEWER_TRANSFERS" }),
    ),
    "subway" => (
      "transitPreferences",
      json!({ "allowedTravelModes": ["SUBWAY"] }),
    ),
    "no-subway" => (
      "transitPreferences",
      json!({ "allowedTravelModes": ["BUS", "TRAIN", "LIGHT_RAIL"] }),
    ),
    "bus" => (
      "transitPreferences",
      json!({ "allowedTravelModes": ["BUS"] }),
    ),
    "rail" => (
      "transitPreferences",
      json!({ "allowedTravelModes": ["RAIL"] }),
    ),
    _ => return Ok(()),
  };
  body[key] = value;
  Ok(())
}

pub async fn route(
  api: &Api,
  from: &str,
  to: &str,
  mode: Mode,
  city: Option<&str>,
  strategy_name: Option<&str>,
) -> Result<Routes> {
  let origin = End::new(api, from, city);
  let destination = End::new(api, to, city);
  let mut body = json!({
    "origin": origin.waypoint(),
    "destination": destination.waypoint(),
    "travelMode": travel_mode(mode),
    "computeAlternativeRoutes": true,
    "units": "METRIC",
  });
  if let Some(name) = strategy_name {
    strategy(&mut body, mode, name)?;
  }
  if let Some(language) = &api.language {
    body["languageCode"] = language.clone().into();
  }
  let url = format!("{ROUTES}/directions/v2:computeRoutes");
  let v = api.post(&url, ROUTE_FIELDS, &body).await?;
  let first = v.at("routes.0.legs.0");
  let last = v.list("routes.0.legs").last().unwrap_or(first);
  Ok(Routes {
    mode,
    origin: origin.located(refs::point(first.at("startLocation.latLng"))),
    destination: destination.located(refs::point(last.at("endLocation.latLng"))),
    routes: v.list("routes").iter().map(parse_route).collect(),
    extra: Default::default(),
  })
}

fn parse_route(r: &Value) -> Route {
  let start = refs::point(r.at("legs.0.startLocation.latLng"));
  let polyline = r.str("polyline.encodedPolyline").map(|line| {
    let system = start.map_or(atp_core::geo::System::Wgs84, |c| c.system);
    Polyline::encoded(system, line)
  });
  let labels: Vec<String> = r
    .list("routeLabels")
    .iter()
    .filter_map(|l| l.as_str().map(label))
    .collect();
  let mut route = Route {
    distance_m: r.u64("distanceMeters"),
    duration_s: parse::seconds(r, "duration"),
    label: r.str("description").or_else(|| labels.first().cloned()),
    fare: parse::money(r.at("travelAdvisory.transitFare")),
    steps: r
      .list("legs")
      .iter()
      .flat_map(|leg| leg.list("steps").iter().map(step))
      .collect(),
    polyline,
    ..Route::default()
  };
  if !labels.is_empty() {
    route.extra.insert("labels".into(), labels.into());
  }
  route
}

fn label(name: &str) -> String {
  match name {
    "DEFAULT_ROUTE" => "recommended".into(),
    "DEFAULT_ROUTE_ALTERNATE" => "alternative".into(),
    "FUEL_EFFICIENT" => "fuel-efficient".into(),
    "SHORTER_DISTANCE" => "shorter".into(),
    other => other.to_lowercase(),
  }
}

fn step(s: &Value) -> Step {
  let t = s.at("transitDetails");
  let mut step = Step {
    instruction: s
      .str("navigationInstruction.instructions")
      .unwrap_or_default(),
    distance_m: s.u64("distanceMeters"),
    duration_s: parse::seconds(s, "staticDuration"),
    ..Step::default()
  };
  if let Some(maneuver) = s.str("navigationInstruction.maneuver") {
    step
      .extra
      .insert("maneuver".into(), maneuver.to_lowercase().into());
  }
  if t.is_null() {
    if s.str("travelMode").as_deref() == Some("WALK") {
      step.mode = Some("walk".into());
    }
    return step;
  }
  let line = t
    .first_str(&["transitLine.nameShort", "transitLine.name"])
    .unwrap_or_default();
  let from = t.str("stopDetails.departureStop.name").unwrap_or_default();
  let to = t.str("stopDetails.arrivalStop.name").unwrap_or_default();
  let stops = t
    .u64("stopCount")
    .map_or(String::new(), |n| format!(", {n} stops"));
  step.instruction = format!("{line}: {from} → {to}{stops}");
  step.mode = t
    .str("transitLine.vehicle.type")
    .map(|v| vehicle(&v).into());
  if let Some(headsign) = t.str("headsign") {
    step.extra.insert("headsign".into(), headsign.into());
  }
  step
}

fn vehicle(kind: &str) -> &'static str {
  match kind {
    "BUS" | "INTERCITY_BUS" | "TROLLEYBUS" | "SHARE_TAXI" => "bus",
    "SUBWAY" | "METRO_RAIL" | "MONORAIL" => "subway",
    "TRAM" => "tram",
    "FERRY" => "ferry",
    _ => "rail",
  }
}

/// Distances from each place to each destination: computeRouteMatrix, or
/// great-circle lines for `straight` (geocoding names first).
pub async fn distance(
  api: &Api,
  from: &[String],
  to: &[String],
  measure: Measure,
) -> Result<Vec<Distance>> {
  let origins: Vec<End> = from.iter().map(|a| End::new(api, a, None)).collect();
  let destinations: Vec<End> = to.iter().map(|a| End::new(api, a, None)).collect();
  if measure == Measure::Straight {
    return straight(api, &origins, &destinations).await;
  }
  let mode = match measure {
    Measure::Walking => "WALK",
    Measure::Cycling => "BICYCLE",
    Measure::Ebike => "TWO_WHEELER",
    _ => "DRIVE",
  };
  let body = json!({
    "origins": origins.iter().map(|o| json!({ "waypoint": o.waypoint() })).collect::<Vec<_>>(),
    "destinations": destinations.iter().map(|d| json!({ "waypoint": d.waypoint() })).collect::<Vec<_>>(),
    "travelMode": mode,
  });
  let url = format!("{ROUTES}/distanceMatrix/v2:computeRouteMatrix");
  let v = api.post(&url, MATRIX_FIELDS, &body).await?;
  let mut cells: Vec<&Value> = v.as_array().map(|a| a.iter().collect()).unwrap_or_default();
  cells.sort_by_key(|c| (c.u64("originIndex"), c.u64("destinationIndex")));
  let found = |c: &&Value| c.str("condition").as_deref() == Some("ROUTE_EXISTS");
  let list = cells.into_iter().filter(found).filter_map(|c| {
    let o = origins.get(c.u64("originIndex").unwrap_or(0) as usize)?;
    let d = destinations.get(c.u64("destinationIndex").unwrap_or(0) as usize)?;
    Some(Distance {
      from: o.located(None),
      to: d.located(None),
      mode: measure,
      distance_m: Some(c.u64("distanceMeters").unwrap_or(0)),
      duration_s: parse::seconds(c, "duration"),
    })
  });
  Ok(list.collect())
}

async fn straight(api: &Api, origins: &[End], destinations: &[End]) -> Result<Vec<Distance>> {
  let mut to = Vec::new();
  for d in destinations {
    to.push((d, geocoding::locate(api, d).await?));
  }
  let mut out = Vec::new();
  for o in origins {
    let from = geocoding::locate(api, o).await?;
    for (d, at) in &to {
      out.push(Distance {
        from: o.located(Some(from)),
        to: d.located(Some(*at)),
        mode: Measure::Straight,
        distance_m: Some(from.distance_m(at).round() as u64),
        duration_s: None,
      });
    }
  }
  Ok(out)
}
