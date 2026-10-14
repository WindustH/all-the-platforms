//! Request signatures.
//!
//! [`web`]: the `sign` of the calls www.dewu.com makes to `app.dewu.com`,
//! as the page's request helper computes it (`c()` in webpack module
//! `d0RG`): the parameters sorted by name, each written as name + value,
//! a fixed salt appended, MD5 in lowercase hex. Arrays are sorted in place
//! first (JavaScript's default `sort`), so the body carries the order the
//! signature saw; a `null` stands for the page's `NaN`, which is signed as
//! an empty value and sent as `null`.
//!
//! [`open`]: the Open Platform signature: the parameters without `sign`
//! and nulls, sorted by name, each name and value Java-URL-encoded, joined
//! as `k=v&k=v`, the app secret appended, MD5 in uppercase hex.

use std::cmp::Ordering;

use atp_core::Value;
use md5::{Digest, Md5};
use serde_json::Map;

/// The salt of the PC, H5 and merchant web pages.
const SALT: &str = "048a9c4943398714b356a696503d2d36";

/// The web `sign` of `params`; sorts their arrays as the page does.
pub fn web(params: &mut Map<String, Value>) -> String {
  for value in params.values_mut() {
    if let Value::Array(items) = value {
      items.sort_by(|a, b| js_order(&sort_key(a), &sort_key(b)));
    }
  }
  let mut names: Vec<&String> = params.keys().collect();
  names.sort_by(|a, b| js_order(a, b));
  let mut text = String::new();
  for name in names {
    text.push_str(name);
    text.push_str(&web_value(&params[name.as_str()]));
  }
  text.push_str(SALT);
  format!("{:x}", Md5::digest(text.as_bytes()))
}

/// What follows a name: nothing for `NaN` and empty arrays, the elements
/// joined by commas, objects as JSON, scalars as text.
fn web_value(value: &Value) -> String {
  match value {
    Value::Null => String::new(),
    Value::Array(items) => join(items),
    Value::Object(_) => value.to_string(),
    other => js_text(other),
  }
}

/// `items.map(o => o instanceof Object ? JSON.stringify(o) : o).reduce((t,
/// e) => t + (t ? "," : "") + e)`: no comma after a falsy first element.
fn join(items: &[Value]) -> String {
  let Some((first, rest)) = items.split_first() else {
    return String::new();
  };
  let mut text = element(first);
  let mut truthy = match first {
    Value::Null => false,
    Value::Bool(b) => *b,
    Value::Number(n) => n.as_f64() != Some(0.0),
    Value::String(s) => !s.is_empty(),
    _ => true,
  };
  for item in rest {
    if truthy {
      text.push(',');
    }
    text.push_str(&element(item));
    truthy = !text.is_empty();
  }
  text
}

fn element(value: &Value) -> String {
  match value {
    Value::Array(_) | Value::Object(_) => value.to_string(),
    other => js_text(other),
  }
}

/// `String(value)` for scalars.
fn js_text(value: &Value) -> String {
  match value {
    Value::String(s) => s.clone(),
    Value::Number(n) => match n.as_f64() {
      Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 1e21 => format!("{f:.0}"),
      _ => n.to_string(),
    },
    Value::Null => "null".into(),
    other => other.to_string(),
  }
}

/// What JavaScript's default `sort` compares: the element as a string.
fn sort_key(value: &Value) -> String {
  match value {
    Value::Object(_) => "[object Object]".into(),
    Value::Array(items) => items.iter().map(js_text).collect::<Vec<_>>().join(","),
    other => js_text(other),
  }
}

/// String order by UTF-16 code units, as JavaScript compares strings.
fn js_order(a: &str, b: &str) -> Ordering {
  a.encode_utf16().cmp(b.encode_utf16())
}

/// The Open Platform `sign` of `params` with the app secret.
pub fn open(params: &Map<String, Value>, secret: &str) -> String {
  let mut pairs: Vec<(&String, &Value)> = params
    .iter()
    .filter(|(name, value)| !value.is_null() && *name != "sign" && *name != "secret")
    .collect();
  pairs.sort_by(|a, b| a.0.cmp(b.0));
  let text = pairs
    .iter()
    .map(|(name, value)| format!("{}={}", java_encode(name), java_encode(&open_value(value))))
    .collect::<Vec<_>>()
    .join("&");
  format!("{:X}", Md5::digest(format!("{text}{secret}").as_bytes()))
}

/// A value as the official SDK writes it: arrays comma-joined (objects in
/// them as JSON), objects as compact JSON without null fields, in their
/// own order.
fn open_value(value: &Value) -> String {
  match value {
    Value::Null => String::new(),
    Value::String(s) => s.clone(),
    Value::Array(items) => items
      .iter()
      .map(|item| match item {
        Value::Array(_) | Value::Object(_) => item.to_string(),
        other => open_value(other),
      })
      .collect::<Vec<_>>()
      .join(","),
    Value::Object(map) => {
      let kept: Map<String, Value> = map
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
      Value::Object(kept).to_string()
    }
    other => other.to_string(),
  }
}

/// Java's `URLEncoder.encode(s, "UTF-8")`: letters, digits and `-_.*`
/// stay, spaces become `+`, every other byte `%XX`.
fn java_encode(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  for byte in text.bytes() {
    match byte {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => out.push(byte as char),
      b' ' => out.push('+'),
      _ => out.push_str(&format!("%{byte:02X}")),
    }
  }
  out
}
