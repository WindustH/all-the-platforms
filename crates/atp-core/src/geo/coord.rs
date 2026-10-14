//! Coordinates and the systems they are in.

use serde::Serialize;

use super::{china, datum};
use crate::error::{Error, Result};

/// A coordinate system (datum) of map platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum System {
  /// WGS-84: GPS, and Google outside mainland China
  Wgs84,
  /// GCJ-02: Amap, Tencent, and Google inside mainland China
  Gcj02,
  /// BD-09: Baidu
  Bd09,
}

impl System {
  pub fn name(self) -> &'static str {
    match self {
      Self::Wgs84 => "wgs84",
      Self::Gcj02 => "gcj02",
      Self::Bd09 => "bd09",
    }
  }

  pub fn parse(name: &str) -> Option<Self> {
    match name {
      "wgs84" => Some(Self::Wgs84),
      "gcj02" => Some(Self::Gcj02),
      "bd09" => Some(Self::Bd09),
      _ => None,
    }
  }
}

/// A point, in the system its numbers are in. Serialized as `{lat, lng,
/// system}`; numbers keep six decimals (about 0.1 m).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Coord {
  pub lat: f64,
  pub lng: f64,
  pub system: System,
}

impl Coord {
  pub fn new(lng: f64, lat: f64, system: System) -> Self {
    Self {
      lat: round6(lat),
      lng: round6(lng),
      system,
    }
  }

  /// The same place in another system.
  pub fn to(self, system: System) -> Self {
    if self.system == system {
      return self;
    }
    let (lng, lat) = datum::convert(self.lng, self.lat, self.system, system);
    Self::new(lng, lat, system)
  }

  /// `LNG,LAT` (as Amap and users write points) in `system`.
  pub fn parse(arg: &str, system: System) -> Option<Self> {
    let (lng, lat) = arg.trim().split_once(',')?;
    let lng: f64 = lng.trim().parse().ok()?;
    let lat: f64 = lat.trim().parse().ok()?;
    let valid = (-180.0..=180.0).contains(&lng) && (-90.0..=90.0).contains(&lat);
    valid.then(|| Self::new(lng, lat, system))
  }

  /// [`Coord::parse`], failing with the expected form.
  pub fn expect(arg: &str, system: System) -> Result<Self> {
    Self::parse(arg, system).ok_or_else(|| {
      Error::input(format!(
        "not a point: `{arg}` (expected LNG,LAT in {}, e.g. 116.397428,39.90923)",
        system.name()
      ))
    })
  }

  /// `lng,lat` with at most six decimals.
  pub fn lng_lat(&self) -> String {
    format!("{},{}", decimals(self.lng), decimals(self.lat))
  }

  /// `lat,lng` with at most six decimals.
  pub fn lat_lng(&self) -> String {
    format!("{},{}", decimals(self.lat), decimals(self.lng))
  }

  /// Great-circle distance in metres (WGS-84 sphere; the systems differ by
  /// far less than the precision this needs).
  pub fn distance_m(&self, other: &Coord) -> f64 {
    let other = other.to(self.system);
    let (p1, p2) = (self.lat.to_radians(), other.lat.to_radians());
    let dp = p2 - p1;
    let dl = (other.lng - self.lng).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6_371_008.8 * a.sqrt().asin()
  }

  /// Inside mainland China, where Google's maps use GCJ-02: not Hong
  /// Kong, Macao or Taiwan. Decided on a 0.25° grid, so points within a
  /// few kilometres of a land border may come out either way.
  pub fn in_mainland_china(&self) -> bool {
    let wgs = self.to(System::Wgs84);
    china::mainland(wgs.lng, wgs.lat)
  }
}

fn round6(x: f64) -> f64 {
  (x * 1e6).round() / 1e6
}

/// Six decimals without trailing zeros.
pub(super) fn decimals(x: f64) -> String {
  let s = format!("{x:.6}");
  s.trim_end_matches('0').trim_end_matches('.').to_owned()
}
