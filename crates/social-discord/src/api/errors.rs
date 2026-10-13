//! Discord's answers: JSON or a shared error, rate-limit waits, and the
//! detail of "Invalid Form Body" errors.

use std::time::Duration;

use social_core::http::Resp;
use social_core::{Error, ErrorCode, Result, Value, ValueExt};

use super::{Api, Token};

impl Api {
  /// Response → JSON (`Null` for empty bodies), or a shared error.
  pub(super) fn check(&self, token: Option<&Token>, resp: Resp) -> Result<Value> {
    let status = resp.status.as_u16();
    if resp.status.is_success() {
      if resp.body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Null);
      }
      return resp.value();
    }
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    let message = v.str("message").unwrap_or_else(|| format!("HTTP {status}"));
    let code = v.i64("code").unwrap_or_default();
    if status == 401 || code == 40001 {
      let hint = if self.env.is_some() {
        "check DISCORD_TOKEN / DISCORD_BOT_TOKEN"
      } else if token.is_some() {
        "the token may have been reset (password change, logout); log in again"
      } else {
        "run `social discord login`"
      };
      return Err(
        Error::auth(format!("Discord did not accept the token ({message})")).with_hint(hint),
      );
    }
    if v.at("captcha_key") != &Value::Null {
      return Err(
        Error::new(
          ErrorCode::VerificationRequired,
          "Discord asks for a captcha for this action",
        )
        .with_hint(
          "do the same once in the Discord app or website, then retry; bots never get captchas",
        ),
      );
    }
    Err(error(&v, status, code, message))
  }
}

/// Discord's `{code, message}` (and the HTTP status) as a shared error.
fn error(v: &Value, status: u16, code: i64, message: String) -> Error {
  let text = if code > 0 {
    format!("{message} (code {code})")
  } else {
    message
  };
  match code {
    10003 | 10004 | 10006 | 10007 | 10008 | 10013 | 10014 | 10015 => Error::not_found(text),
    40002 => Error::new(ErrorCode::VerificationRequired, text)
      .with_hint("verify the account's email or phone in the Discord app"),
    20001 | 20002 => Error::new(ErrorCode::UnsupportedOperation, text),
    20016 | 20028 | 31001 => Error::new(ErrorCode::RateLimited, text),
    50035 => Error::input(match first_form_error(v) {
      Some(detail) => format!("{text}: {detail}"),
      None => text,
    }),
    50001 | 50005 | 50007 | 50013 | 50021 | 50024 | 160002 => {
      Error::new(ErrorCode::PermissionDenied, text)
    }
    _ => match status {
      403 => Error::new(ErrorCode::PermissionDenied, text),
      404 => Error::not_found(text),
      413 => Error::input(format!("{text}: the file is too large for this channel")),
      429 => Error::new(ErrorCode::RateLimited, text),
      400 => Error::input(text),
      _ => Error::upstream(format!("Discord: {text} (HTTP {status})")),
    },
  }
}

pub(super) fn retry_after(resp: &Resp) -> Option<f64> {
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
  v.f64("retry_after")
    .or_else(|| resp.header("retry-after").and_then(|s| s.parse().ok()))
}

pub(super) async fn sleep_secs(secs: f64) {
  let jitter: f64 = rand::random_range(0.2..0.8);
  tokio::time::sleep(Duration::from_secs_f64(secs.max(0.0) + jitter)).await;
}

/// The first message inside `errors` of an "Invalid Form Body" answer.
fn first_form_error(v: &Value) -> Option<String> {
  fn walk(v: &Value, path: &str) -> Option<String> {
    if let Some(e) = v.list("_errors").first() {
      let message = e.str("message")?;
      return Some(if path.is_empty() {
        message
      } else {
        format!("{path}: {message}")
      });
    }
    v.as_object()?.iter().find_map(|(k, child)| {
      let path = if path.is_empty() {
        k.clone()
      } else {
        format!("{path}.{k}")
      };
      walk(child, &path)
    })
  }
  walk(v.at("errors"), "")
}
