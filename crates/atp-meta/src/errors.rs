//! Graph answers as errors. Errors arrive as `{"error": {"message", "type",
//! "code", "error_subcode", "error_user_msg", "fbtrace_id"}}`; usage against
//! the rate limits comes in `X-App-Usage` / `X-Business-Use-Case-Usage` /
//! `X-Ad-Account-Usage` headers (percentages), logged and warned about near
//! the limit.

use atp_core::http::{Resp, status_error};
use atp_core::{Error, ErrorCode, Result, Value, ValueExt};

/// The body as JSON, or the Graph error it carries mapped to an error code.
pub fn check(resp: Resp) -> Result<Value> {
  usage(&resp);
  let v: Value = resp.value().unwrap_or_else(|_| Value::String(resp.text()));
  let Some(err) = v.get("error").filter(|e| e.is_object()) else {
    if !resp.status.is_success() {
      return Err(status_error(resp.status, &resp.text()));
    }
    return Ok(v);
  };
  Err(error(err))
}

/// A Graph error object as an [`Error`].
pub fn error(err: &Value) -> Error {
  let code = err.i64("code").unwrap_or(0);
  let subcode = err.i64("error_subcode").unwrap_or(0);
  let text = describe(err, code, subcode);
  match code {
    104 | 190 | 102 | 2500 => Error::auth(text).with_hint(
      "the access token is invalid or expired: run `login --token` with a new one or update the environment variable",
    ),
    4 | 17 | 32 | 613 | 80000..=80014 => Error::new(ErrorCode::RateLimited, text)
      .with_hint("wait (usually an hour at most) and slow down; usage is in the X-App-Usage headers"),
    368 => Error::new(ErrorCode::PermissionDenied, text)
      .with_hint("Meta temporarily blocked this action for policy reasons"),
    10 | 200..=299 => Error::new(ErrorCode::PermissionDenied, text)
      .with_hint("the token lacks a permission for this call (check its scopes, and the app's review status)"),
    803 => Error::not_found(text),
    100 if subcode == 33 => Error::not_found(text),
    100 => Error::input(text),
    _ => Error::upstream(text),
  }
}

/// `Graph API error CODE[/SUBCODE]: message (fbtrace_id …)`.
fn describe(err: &Value, code: i64, subcode: i64) -> String {
  let message = err
    .str("error_user_msg")
    .or_else(|| err.str("message"))
    .unwrap_or_else(|| "unknown error".into());
  let mut text = format!("Graph API error {code}");
  if subcode != 0 {
    text += &format!("/{subcode}");
  }
  text += &format!(": {message}");
  if let Some(trace) = err.str("fbtrace_id") {
    text += &format!(" (fbtrace_id {trace})");
  }
  text
}

/// Log the rate-limit usage headers; warn above 90 %.
fn usage(resp: &Resp) {
  for name in [
    "x-app-usage",
    "x-business-use-case-usage",
    "x-ad-account-usage",
  ] {
    let Some(raw) = resp.header(name) else {
      continue;
    };
    tracing::debug!("{name}: {raw}");
    let high = serde_json::from_str::<Value>(raw)
      .ok()
      .map(|v| max_percent(&v))
      .unwrap_or(0.0);
    if high >= 90.0 {
      tracing::warn!("{name} at {high:.0}% of Meta's rate limit; slow down");
    }
  }
}

fn max_percent(v: &Value) -> f64 {
  match v {
    Value::Number(n) => n.as_f64().unwrap_or(0.0),
    Value::Object(m) => m.values().map(max_percent).fold(0.0, f64::max),
    Value::Array(a) => a.iter().map(max_percent).fold(0.0, f64::max),
    _ => 0.0,
  }
}
