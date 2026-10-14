//! The shape of a route, compact: Google's encoded polyline format
//! (precision 1e-5, about a metre), which most map tools decode, with the
//! system its points are in.

use serde::Serialize;

use super::{Coord, System};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Polyline {
  pub system: System,
  /// Encoded polyline (precision 5) of `lat,lng` points.
  pub encoded: String,
}

impl Polyline {
  /// `points` as `(lng, lat)`.
  pub fn new(system: System, points: &[(f64, f64)]) -> Self {
    let mut encoded = String::new();
    let (mut last_lat, mut last_lng) = (0i64, 0i64);
    for &(lng, lat) in points {
      let (lat, lng) = ((lat * 1e5).round() as i64, (lng * 1e5).round() as i64);
      push(&mut encoded, lat - last_lat);
      push(&mut encoded, lng - last_lng);
      (last_lat, last_lng) = (lat, lng);
    }
    Self { system, encoded }
  }

  /// A polyline as Google sends it, already encoded.
  pub fn encoded(system: System, encoded: impl Into<String>) -> Self {
    Self {
      system,
      encoded: encoded.into(),
    }
  }

  /// `lng,lat;lng,lat;…` (Amap and Baidu paths); `|` separates parts too.
  pub fn from_path(system: System, path: &str) -> Self {
    Self::new(system, &path_points(path))
  }

  /// The points as `(lng, lat)`.
  pub fn points(&self) -> Vec<(f64, f64)> {
    let mut values = Vec::new();
    let (mut value, mut shift) = (0i64, 0);
    for b in self.encoded.bytes() {
      let chunk = i64::from(b) - 63;
      value |= (chunk & 0x1f) << shift;
      shift += 5;
      if chunk < 0x20 {
        values.push(if value & 1 == 1 {
          !(value >> 1)
        } else {
          value >> 1
        });
        (value, shift) = (0, 0);
      }
    }
    let (mut lat, mut lng) = (0i64, 0i64);
    let (pairs, _) = values.as_chunks::<2>();
    pairs
      .iter()
      .map(|[d_lat, d_lng]| {
        lat += d_lat;
        lng += d_lng;
        (lng as f64 / 1e5, lat as f64 / 1e5)
      })
      .collect()
  }

  /// The same line in another system.
  pub fn to(&self, system: System) -> Self {
    if self.system == system {
      return self.clone();
    }
    let points: Vec<(f64, f64)> = self
      .points()
      .into_iter()
      .map(|(lng, lat)| {
        let c = Coord::new(lng, lat, self.system).to(system);
        (c.lng, c.lat)
      })
      .collect();
    Self::new(system, &points)
  }
}

/// Points of a `lng,lat;lng,lat` path; malformed pairs are skipped.
pub fn path_points(path: &str) -> Vec<(f64, f64)> {
  path
    .split([';', '|'])
    .filter_map(|pair| {
      let (lng, lat) = pair.split_once(',')?;
      Some((lng.trim().parse().ok()?, lat.trim().parse().ok()?))
    })
    .collect()
}

fn push(out: &mut String, value: i64) {
  let mut v = if value < 0 { !(value << 1) } else { value << 1 };
  while v >= 0x20 {
    out.push(char::from((0x20 | (v & 0x1f)) as u8 + 63));
    v >>= 5;
  }
  out.push(char::from(v as u8 + 63));
}
