//! Every coordinate of a result, in the system `--coord` asks for.
//!
//! Typed coordinates (places, routes, distances) are converted directly;
//! inside JSON (`extra`, values) a coordinate is an object with numeric
//! `lat` and `lng` and a `system` name, the way [`Coord`] serializes.

use serde_json::Value;

use super::{Coord, Routes, System, Waypoint};
use crate::model::{Data, Extra, Post};

pub fn localize(data: &mut Data, system: System) {
  match data {
    Data::Posts(page) => page.items.iter_mut().for_each(|p| post(p, system)),
    Data::Post(p) => post(p, system),
    Data::Collections(page) => page
      .items
      .iter_mut()
      .for_each(|c| extra(&mut c.extra, system)),
    Data::Routes(r) => routes(r, system),
    Data::Distances(list) => list.iter_mut().for_each(|d| {
      waypoint(&mut d.from, system);
      waypoint(&mut d.to, system);
    }),
    Data::Value(v) => value(v, system),
    _ => {}
  }
}

fn coord(c: &mut Option<Coord>, system: System) {
  if let Some(c) = c {
    *c = c.to(system);
  }
}

fn post(p: &mut Post, system: System) {
  if let Some(place) = &mut p.place {
    coord(&mut place.location, system);
  }
  extra(&mut p.extra, system);
  if let Some(q) = &mut p.quoted {
    post(q, system);
  }
}

fn waypoint(w: &mut Waypoint, system: System) {
  coord(&mut w.location, system);
}

fn routes(r: &mut Routes, system: System) {
  waypoint(&mut r.origin, system);
  waypoint(&mut r.destination, system);
  extra(&mut r.extra, system);
  for route in &mut r.routes {
    if let Some(line) = &mut route.polyline {
      *line = line.to(system);
    }
    extra(&mut route.extra, system);
    for step in &mut route.steps {
      extra(&mut step.extra, system);
    }
  }
}

fn extra(map: &mut Extra, system: System) {
  map.values_mut().for_each(|v| value(v, system));
}

fn value(v: &mut Value, system: System) {
  if let Some(c) = as_coord(v) {
    if let Ok(converted) = serde_json::to_value(c.to(system)) {
      *v = converted;
    }
    return;
  }
  match v {
    Value::Array(items) => items.iter_mut().for_each(|x| value(x, system)),
    Value::Object(map) => map.values_mut().for_each(|x| value(x, system)),
    _ => {}
  }
}

fn as_coord(v: &Value) -> Option<Coord> {
  let map = v.as_object()?;
  let system = System::parse(map.get("system")?.as_str()?)?;
  let lat = map.get("lat")?.as_f64()?;
  let lng = map.get("lng")?.as_f64()?;
  Some(Coord::new(lng, lat, system))
}
