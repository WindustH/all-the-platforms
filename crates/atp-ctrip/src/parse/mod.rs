//! Ctrip answers and pages → core model. Pages carry their data as JSON:
//! the Next.js pages in `__NEXT_DATA__` (sights, rankings, trains), the
//! App Router pages in React Server Component chunks
//! (`self.__next_f.push([1,"…"])`: hotels, travel notes).

mod community;
mod fares;
mod hotels;
mod reviews;
mod sights;
mod trains;

use std::sync::LazyLock;

use atp_core::text::{from_beijing, from_millis};
use atp_core::{Extra, Media, Value, ValueExt};
use jiff::Timestamp;
use regex::Regex;

pub use community::{mix_page, note, note_comment, travel, user};
pub use fares::fare;
pub use hotels::{hotel, hotel_item, room};
pub use reviews::{hotel_review, sight_review};
pub use sights::{ranking_item, search_item, sight_card, sight_more, sight_page, suggestion};
pub use trains::{train, transfer};

static DATE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"/Date\((-?\d+)(?:[+-]\d{4})?\)/").expect("valid"));
static SIZED: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(
    r"^(https?://[^?#]+?/images/[^?#]+?)_[A-Z]{1,2}_\d+[^./?#]*(\.[A-Za-z]+)(?:_\.webp)?(?:\?.*)?$",
  )
  .expect("valid")
});

pub use atp_core::json::next_data;

/// The value of `"key":` in a page's React Server Component chunks.
pub fn rsc(html: &str, key: &str) -> Option<Value> {
  let text = rsc_text(html);
  let at = text.find(&format!("\"{key}\":"))? + key.len() + 3;
  first_value(&text[at..])
}

/// The object starting with `prefix` in the page's RSC chunks.
pub fn rsc_object(html: &str, prefix: &str) -> Option<Value> {
  let text = rsc_text(html);
  first_value(&text[text.find(prefix)?..])
}

/// The RSC chunks of a page, joined and unescaped.
fn rsc_text(html: &str) -> String {
  const PUSH: &str = "self.__next_f.push([1,";
  let mut text = String::new();
  let mut rest = html;
  while let Some(at) = rest.find(PUSH) {
    rest = &rest[at + PUSH.len()..];
    if let Some(Value::String(chunk)) = first_value(rest) {
      text.push_str(&chunk);
    }
  }
  text
}

fn first_value(text: &str) -> Option<Value> {
  serde_json::Deserializer::from_str(text.trim_start())
    .into_iter::<Value>()
    .next()?
    .ok()
}

/// A time as Ctrip writes it: `/Date(1791814989000+0800)/`, epoch
/// milliseconds, or Beijing wall-clock time.
pub fn time(v: &Value, path: &str) -> Option<Timestamp> {
  match v.at(path) {
    Value::Number(n) => n.as_i64().and_then(from_millis),
    Value::String(s) => DATE
      .captures(s)
      .and_then(|m| m[1].parse().ok())
      .and_then(from_millis)
      .or_else(|| from_beijing(s)),
    _ => None,
  }
}

/// The full-size image behind a resized one
/// (`…/images/<id>_R_600_600_R5_D.jpg_.webp` → `…/images/<id>.jpg`).
pub fn original(url: &str) -> String {
  let url = absolute(url);
  match SIZED.captures(&url) {
    Some(m) => format!("{}{}", &m[1], &m[2]),
    None => url,
  }
}

/// `//host/…` and `http://…` links as `https://`.
pub fn absolute(link: &str) -> String {
  let link = link.trim();
  if let Some(rest) = link.strip_prefix("//") {
    format!("https://{rest}")
  } else {
    link.replacen("http://", "https://", 1)
  }
}

/// Images at `path` (objects with one of `keys`, or plain links), full size.
pub fn images(v: &Value, path: &str, keys: &[&str]) -> Vec<Media> {
  v.list(path)
    .iter()
    .filter_map(|i| i.as_str().map(str::to_owned).or_else(|| i.first_str(keys)))
    .map(|u| Media::image(original(&u)))
    .collect()
}

/// Put `value` into `extra` unless it is empty.
pub fn put(extra: &mut Extra, key: &str, value: Option<Value>) {
  let empty = |x: &Value| {
    x.is_null()
      || x.as_str().is_some_and(str::is_empty)
      || x.as_array().is_some_and(Vec::is_empty)
      || x.as_object().is_some_and(|o| o.is_empty())
  };
  if let Some(value) = value.filter(|x| !empty(x)) {
    extra.insert(key.into(), value);
  }
}

/// Copy the values at `fields` (name in `extra`, path in `v`) into
/// `extra`, leaving out empty ones.
pub fn copy(extra: &mut Extra, v: &Value, fields: &[(&str, &str)]) {
  for (key, path) in fields {
    put(extra, key, Some(v.at(path).clone()));
  }
}

/// Copy the ids at `fields` into `extra` as strings, leaving out `0`.
pub fn ids(extra: &mut Extra, v: &Value, fields: &[(&str, &str)]) {
  for (key, path) in fields {
    put(
      extra,
      key,
      v.str(path).filter(|id| id != "0").map(Value::from),
    );
  }
}

/// Strings at `path` (an array of strings, or of objects with `key`).
pub fn strings(v: &Value, path: &str, key: &str) -> Vec<String> {
  v.list(path)
    .iter()
    .filter_map(|x| x.as_str().map(str::to_owned).or_else(|| x.str(key)))
    .filter(|s| !s.trim().is_empty())
    .collect()
}

/// `lng,lat` of a coordinate object, with its system in `extra`.
pub fn location(extra: &mut Extra, v: &Value, lat: &str, lng: &str, system: Option<String>) {
  let (Some(lat), Some(lng)) = (v.f64(lat), v.f64(lng)) else {
    return;
  };
  if lat == 0.0 && lng == 0.0 {
    return;
  }
  put(extra, "location", Some(format!("{lng},{lat}").into()));
  put(extra, "coord_type", system.map(Value::from));
}
