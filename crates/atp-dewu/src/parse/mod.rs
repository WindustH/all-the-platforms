//! Dewu payloads → core model. Products are posts (`kind` `item`) with
//! their lowest price in `price` and their sizes in `extra.sizes`;
//! community posts are posts of kind `video` or `post`.

mod community;
mod open;
mod web;

use std::sync::LazyLock;

use atp_core::{Extra, Media, Value, ValueExt};
use regex::Regex;

pub use self::community::*;
pub use self::open::*;
pub use self::web::*;

static SIZE_IN_URL: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"_w(\d{2,5})_?h(\d{2,5})").expect("valid"));

/// Keep a value unless it is null, empty or an empty list.
fn insert(extra: &mut Extra, key: &str, value: Option<Value>) {
  let empty = |v: &Value| match v {
    Value::Null => true,
    Value::String(s) => s.is_empty(),
    Value::Array(a) => a.is_empty(),
    Value::Object(o) => o.is_empty(),
    _ => false,
  };
  if let Some(value) = value.filter(|v| !empty(v)) {
    extra.insert(key.into(), value);
  }
}

/// An amount in fen (or another currency's minor unit) as main units;
/// zero and negative amounts mean none.
fn minor(v: &Value, path: &str) -> Option<f64> {
  v.f64(path).filter(|n| *n > 0.0).map(|n| n / 100.0)
}

/// A JSON number of an amount (`499` rather than `499.0`).
fn amount(n: f64) -> Value {
  if n.fract() == 0.0 && n.abs() < 9e15 {
    Value::from(n as i64)
  } else {
    Value::from(n)
  }
}

/// An image or video with the size its CDN name carries (`_w720h1280`).
fn media(mut media: Media) -> Media {
  if let Some(m) = SIZE_IN_URL.captures(&media.url) {
    media.width = m[1].parse().ok();
    media.height = m[2].parse().ok();
  }
  media
}
