//! Places, routes and distances as every map platform prints them.

use serde::Serialize;

use super::{Coord, Polyline};
use crate::model::{Extra, Price};

/// Where a place is and what it is: `Post.place` of POIs, addresses and
/// suggestions. The post holds the name (`title`), id, link and photos.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Place {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub location: Option<Coord>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub address: Option<String>,
  /// The platform's category or type, as it words it.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub category: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub phone: Option<String>,
  /// Rating out of 5.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub rating: Option<f64>,
  /// Metres from the point a search was around.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub distance_m: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub country: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub province: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub city: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub district: Option<String>,
  /// China's six-digit administrative code.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub adcode: Option<String>,
  /// Opening hours as the platform words them.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub hours: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub website: Option<String>,
}

impl Place {
  /// "province city district" without repeats (municipalities name the
  /// city twice), or `None` when all are missing.
  pub fn region(&self) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in [&self.province, &self.city, &self.district]
      .into_iter()
      .flatten()
    {
      if !part.is_empty() && !parts.contains(&part.as_str()) {
        parts.push(part);
      }
    }
    (!parts.is_empty()).then(|| parts.join(" "))
  }
}

/// How a route is travelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
  Driving,
  Walking,
  Cycling,
  /// Electric bikes and scooters
  Ebike,
  /// Buses, subways and trains, with walks between them
  Transit,
}

impl Mode {
  pub fn name(self) -> &'static str {
    match self {
      Self::Driving => "driving",
      Self::Walking => "walking",
      Self::Cycling => "cycling",
      Self::Ebike => "ebike",
      Self::Transit => "transit",
    }
  }
}

/// How a distance is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Measure {
  /// A straight line
  Straight,
  Driving,
  Walking,
  Cycling,
  /// Electric bikes and scooters
  Ebike,
}

/// One end of a route or distance: what the user gave, and where it is.
#[derive(Debug, Clone, Serialize)]
pub struct Waypoint {
  pub name: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub location: Option<Coord>,
}

/// A route plan: one or more routes between two places.
#[derive(Debug, Clone, Serialize)]
pub struct Routes {
  pub mode: Mode,
  pub origin: Waypoint,
  pub destination: Waypoint,
  pub routes: Vec<Route>,
  /// Platform fields of the plan (a taxi estimate ...).
  #[serde(skip_serializing_if = "Extra::is_empty")]
  pub extra: Extra,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Route {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub distance_m: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub duration_s: Option<u64>,
  /// What the platform calls this route (`recommended`, `avoids tolls` …).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tolls: Option<Price>,
  /// Tickets of a transit route.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub fare: Option<Price>,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub steps: Vec<Step>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub polyline: Option<Polyline>,
  #[serde(skip_serializing_if = "Extra::is_empty")]
  pub extra: Extra,
}

/// One instruction of a route, or one leg of a transit route.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Step {
  pub instruction: String,
  /// Transit legs: `walk`, `bus`, `subway`, `rail`, `taxi` …
  #[serde(skip_serializing_if = "Option::is_none")]
  pub mode: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub road: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub distance_m: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub duration_s: Option<u64>,
  #[serde(skip_serializing_if = "Extra::is_empty")]
  pub extra: Extra,
}

/// The distance between two places, along roads or in a straight line.
#[derive(Debug, Clone, Serialize)]
pub struct Distance {
  pub from: Waypoint,
  pub to: Waypoint,
  pub mode: Measure,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub distance_m: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub duration_s: Option<u64>,
}
