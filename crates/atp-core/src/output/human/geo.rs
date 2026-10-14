//! Map results: places as a table of names and addresses, a place's
//! details on its card, routes with their steps, distance tables.

use super::*;
use crate::geo::{Coord, Distance, Place, Route, Routes, Waypoint, fmt_distance, fmt_trip};

/// Posts that are all places (search, nearby, tips, geocoding).
pub(super) fn is_places(posts: &[Post]) -> bool {
  !posts.is_empty() && posts.iter().all(|p| p.place.is_some())
}

pub(super) fn place_table(posts: &[Post]) {
  let distance = posts.iter().any(|p| place(p).distance_m.is_some());
  let rating = posts.iter().any(|p| place(p).rating.is_some());
  let mut header = vec!["#", "Name"];
  header.extend(distance.then_some("Distance"));
  header.extend(rating.then_some("Rating"));
  header.extend(["Address", "Category", "ID"]);
  let mut t = table(&header);
  for (i, p) in posts.iter().enumerate() {
    let place = place(p);
    let mut row = vec![index(i), Cell::new(truncate(&headline(p), 32))];
    if distance {
      let d = place.distance_m.map(fmt_distance).unwrap_or_default();
      row.push(Cell::new(d).set_alignment(CellAlignment::Right));
    }
    if rating {
      let r = place.rating.map(|r| format!("{r:.1}")).unwrap_or_default();
      row.push(Cell::new(r).set_alignment(CellAlignment::Right));
    }
    row.extend([
      Cell::new(truncate(&address(place), 48)),
      Cell::new(truncate(place.category.as_deref().unwrap_or(""), 20)),
      Cell::new(truncate(&p.id, 28)).add_attribute(Attribute::Dim),
    ]);
    t.add_row(row);
  }
  println!("{t}");
}

fn place(p: &Post) -> &Place {
  static NONE: Place = Place {
    location: None,
    address: None,
    category: None,
    phone: None,
    rating: None,
    distance_m: None,
    country: None,
    province: None,
    city: None,
    district: None,
    adcode: None,
    hours: None,
    website: None,
  };
  p.place.as_deref().unwrap_or(&NONE)
}

/// The address, or the region when there is none.
fn address(place: &Place) -> String {
  place
    .address
    .clone()
    .or_else(|| place.region())
    .unwrap_or_default()
}

/// A place's details under the title of its card.
pub(super) fn place_lines(place: &Place) {
  // Chinese platforms leave the city and district out of addresses.
  let named = |a: &str| {
    [&place.city, &place.district]
      .into_iter()
      .flatten()
      .any(|part| a.contains(part.as_str()))
  };
  let full = match (place.region(), &place.address) {
    (Some(region), Some(a)) if !named(a) => format!("{region} {a}"),
    (region, a) => a.clone().or(region).unwrap_or_default(),
  };
  if !full.is_empty() {
    println!("{full}");
  }
  let mut meta = Vec::new();
  meta.extend(place.category.clone());
  meta.extend(place.rating.map(|r| format!("★ {r:.1}")));
  meta.extend(
    place
      .distance_m
      .map(|d| format!("{} away", fmt_distance(d))),
  );
  meta.extend(place.location.as_ref().map(point));
  if !meta.is_empty() {
    println!(
      "{}",
      meta
        .join(" · ")
        .if_supports_color(Stream::Stdout, |t| t.dimmed())
    );
  }
  for (label, value) in [
    ("phone", &place.phone),
    ("hours", &place.hours),
    ("web", &place.website),
  ] {
    if let Some(v) = value {
      println!("{label}: {v}");
    }
  }
}

/// `116.397428,39.90923 (gcj02)`.
fn point(c: &Coord) -> String {
  format!("{} ({})", c.lng_lat(), c.system.name())
}

fn waypoint(w: &Waypoint) -> String {
  match &w.location {
    Some(c) if c.lng_lat() != w.name => format!("{} [{}]", w.name, c.lng_lat()),
    _ => w.name.clone(),
  }
}

/// The plan's ends, every route on a line, and the first route's steps.
pub(super) fn routes(r: &Routes) {
  println!(
    "{} {} → {}",
    r.mode
      .name()
      .if_supports_color(Stream::Stdout, |t| t.bold()),
    waypoint(&r.origin),
    waypoint(&r.destination)
  );
  if r.routes.is_empty() {
    note("(no route)");
    return;
  }
  for (i, route) in r.routes.iter().enumerate() {
    println!("{}. {}", i + 1, summary(route));
  }
  let Some(first) = r.routes.first().filter(|f| !f.steps.is_empty()) else {
    return;
  };
  println!();
  let mut t = table(&["#", "Step", "Distance", "Time"]);
  for (i, s) in first.steps.iter().enumerate() {
    let text = match &s.mode {
      Some(mode) if !s.instruction.starts_with(mode.as_str()) => {
        format!("[{mode}] {}", s.instruction)
      }
      _ => s.instruction.clone(),
    };
    t.add_row(vec![
      index(i),
      Cell::new(truncate(&one_line(&text), 72)),
      Cell::new(s.distance_m.map(fmt_distance).unwrap_or_default())
        .set_alignment(CellAlignment::Right),
      Cell::new(s.duration_s.map(fmt_trip).unwrap_or_default()).set_alignment(CellAlignment::Right),
    ]);
  }
  println!("{t}");
  if r.routes.len() > 1 {
    note("steps of route 1; -f yaml shows every route's steps");
  }
}

/// `12.3 km · 25 min · tolls ¥5 · recommended`.
fn summary(route: &Route) -> String {
  let mut parts = Vec::new();
  parts.extend(route.distance_m.map(fmt_distance));
  parts.extend(route.duration_s.map(fmt_trip));
  parts.extend(
    route
      .tolls
      .as_ref()
      .map(|p| format!("tolls {}", p.display())),
  );
  parts.extend(route.fare.as_ref().map(|p| format!("fare {}", p.display())));
  parts.extend(route.label.clone());
  parts.join(" · ")
}

pub(super) fn distances(list: &[Distance]) {
  if list.is_empty() {
    note("(no results)");
    return;
  }
  let mut t = table(&["#", "From", "To", "Distance", "Time"]);
  for (i, d) in list.iter().enumerate() {
    t.add_row(vec![
      index(i),
      Cell::new(truncate(&d.from.name, 32)),
      Cell::new(truncate(&d.to.name, 32)),
      Cell::new(d.distance_m.map(fmt_distance).unwrap_or_default())
        .set_alignment(CellAlignment::Right),
      Cell::new(d.duration_s.map(fmt_trip).unwrap_or_default()).set_alignment(CellAlignment::Right),
    ]);
  }
  println!("{t}");
}
