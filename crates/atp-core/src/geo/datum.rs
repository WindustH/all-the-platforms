//! Conversions between WGS-84, GCJ-02 and BD-09 with the well-known public
//! formulas. GCJ-02 shifts points inside China's rough bounding box only;
//! going back to WGS-84 refines the inverse until it is within about a
//! millimetre.

use std::f64::consts::PI;

use super::System;

/// Krasovsky 1940 ellipsoid, as GCJ-02 uses it.
const A: f64 = 6_378_245.0;
const EE: f64 = 0.006_693_421_622_965_943;
/// BD-09's rotation constant.
const X_PI: f64 = PI * 3000.0 / 180.0;

pub fn convert(lng: f64, lat: f64, from: System, to: System) -> (f64, f64) {
  let gcj = match from {
    System::Wgs84 if to == System::Wgs84 => return (lng, lat),
    System::Wgs84 => wgs84_to_gcj02(lng, lat),
    System::Gcj02 => (lng, lat),
    System::Bd09 => bd09_to_gcj02(lng, lat),
  };
  match to {
    System::Wgs84 => gcj02_to_wgs84(gcj.0, gcj.1),
    System::Gcj02 => gcj,
    System::Bd09 => gcj02_to_bd09(gcj.0, gcj.1),
  }
}

/// Outside the box GCJ-02 shifts (and the inverse leaves alone).
fn out_of_china(lng: f64, lat: f64) -> bool {
  !(72.004..=137.8347).contains(&lng) || !(0.8293..=55.8271).contains(&lat)
}

fn shift(lng: f64, lat: f64) -> (f64, f64) {
  let (x, y) = (lng - 105.0, lat - 35.0);
  let mut d_lat = -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * x.abs().sqrt();
  d_lat += wave(x) + ((20.0 * (y * PI).sin() + 40.0 * (y / 3.0 * PI).sin()) * 2.0 / 3.0);
  d_lat += (160.0 * (y / 12.0 * PI).sin() + 320.0 * (y * PI / 30.0).sin()) * 2.0 / 3.0;
  let mut d_lng = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * x.abs().sqrt();
  d_lng += wave(x) + ((20.0 * (x * PI).sin() + 40.0 * (x / 3.0 * PI).sin()) * 2.0 / 3.0);
  d_lng += (150.0 * (x / 12.0 * PI).sin() + 300.0 * (x / 30.0 * PI).sin()) * 2.0 / 3.0;
  let rad = lat / 180.0 * PI;
  let magic = 1.0 - EE * rad.sin() * rad.sin();
  let sqrt_magic = magic.sqrt();
  let d_lat = d_lat * 180.0 / ((A * (1.0 - EE)) / (magic * sqrt_magic) * PI);
  let d_lng = d_lng * 180.0 / (A / sqrt_magic * rad.cos() * PI);
  (d_lng, d_lat)
}

fn wave(x: f64) -> f64 {
  (20.0 * (6.0 * x * PI).sin() + 20.0 * (2.0 * x * PI).sin()) * 2.0 / 3.0
}

pub fn wgs84_to_gcj02(lng: f64, lat: f64) -> (f64, f64) {
  if out_of_china(lng, lat) {
    return (lng, lat);
  }
  let (d_lng, d_lat) = shift(lng, lat);
  (lng + d_lng, lat + d_lat)
}

/// The inverse, refined until the forward shift lands within 1e-9° of
/// the input.
pub fn gcj02_to_wgs84(lng: f64, lat: f64) -> (f64, f64) {
  if out_of_china(lng, lat) {
    return (lng, lat);
  }
  let (mut w_lng, mut w_lat) = (lng, lat);
  for _ in 0..30 {
    let (g_lng, g_lat) = wgs84_to_gcj02(w_lng, w_lat);
    let (e_lng, e_lat) = (g_lng - lng, g_lat - lat);
    w_lng -= e_lng;
    w_lat -= e_lat;
    if e_lng.abs() < 1e-9 && e_lat.abs() < 1e-9 {
      break;
    }
  }
  (w_lng, w_lat)
}

pub fn gcj02_to_bd09(lng: f64, lat: f64) -> (f64, f64) {
  let z = (lng * lng + lat * lat).sqrt() + 0.00002 * (lat * X_PI).sin();
  let theta = lat.atan2(lng) + 0.000003 * (lng * X_PI).cos();
  (z * theta.cos() + 0.0065, z * theta.sin() + 0.006)
}

pub fn bd09_to_gcj02(lng: f64, lat: f64) -> (f64, f64) {
  let (x, y) = (lng - 0.0065, lat - 0.006);
  let z = (x * x + y * y).sqrt() - 0.00002 * (y * X_PI).sin();
  let theta = y.atan2(x) - 0.000003 * (x * X_PI).cos();
  (z * theta.cos(), z * theta.sin())
}
