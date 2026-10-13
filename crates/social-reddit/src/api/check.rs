//! Responses mapped to JSON or a shared error: rate limits, network blocks,
//! refused credentials, private and banned communities, write errors.

use social_core::http::{Resp, status_error};
use social_core::{Error, ErrorCode, Result, Value, ValueExt};

use super::{Api, BLOCK_HINT};

impl Api {
  /// Response → JSON, or a shared error.
  pub(super) fn check(&self, resp: Resp) -> Result<Value> {
    let status = resp.status.as_u16();
    // Some writes answer `202 Accepted` without a body.
    if resp.status.is_success() && resp.body.iter().all(u8::is_ascii_whitespace) {
      return Ok(Value::Null);
    }
    let json: Option<Value> = serde_json::from_slice(&resp.body).ok();
    match json {
      Some(v) if resp.status.is_success() => return Ok(v),
      None if resp.status.is_success() => return Err(web_page(&resp)),
      _ => {}
    }
    if status == 401 {
      let hint = match &self.oauth {
        Some(o) => {
          o.forget(&self.ctx);
          "the OAuth token was refused; the next run asks for a new one".to_owned()
        }
        None => self.ctx.login_hint(),
      };
      return Err(Error::auth("Reddit did not accept the credentials (HTTP 401)").with_hint(hint));
    }
    Err(match (status, json) {
      (429, _) => Error::new(
        ErrorCode::RateLimited,
        "Reddit rate limit reached (HTTP 429)",
      )
      .with_hint("wait a minute before retrying; logged-in requests get a higher limit"),
      (_, None) if is_block_page(&resp) => blocked(),
      (_, Some(v)) => {
        refused(status, &v).unwrap_or_else(|| status_error(resp.status, &resp.text()))
      }
      _ => status_error(resp.status, &resp.text()),
    })
  }
}

fn is_block_page(resp: &Resp) -> bool {
  let text = resp.text().to_ascii_lowercase();
  text.contains("blocked by network security") || text.contains("blocked due to a network policy")
}

fn blocked() -> Error {
  Error::new(
    ErrorCode::VerificationRequired,
    "Reddit blocked this request (network security)",
  )
  .with_hint(BLOCK_HINT)
}

/// A web page where JSON was expected: a block page or a login redirect.
fn web_page(resp: &Resp) -> Error {
  if is_block_page(resp) {
    return blocked();
  }
  if resp.url.contains("/login") {
    return Error::auth("Reddit asked for a login").with_hint(BLOCK_HINT);
  }
  Error::upstream(format!(
    "unexpected response from {}: {}",
    resp.url.split('?').next().unwrap_or_default(),
    social_core::text::truncate(&social_core::text::one_line(&resp.text()), 120)
  ))
}

/// `{"reason": "private", "message": "Forbidden", "error": 403}` and friends.
fn refused(status: u16, v: &Value) -> Option<Error> {
  let reason = v.str("reason")?;
  let denied = |message: &str| Error::new(ErrorCode::PermissionDenied, message);
  Some(match reason.as_str() {
    "private" => denied("this community is private"),
    "quarantined" | "gated" => denied(&format!("this community is {reason}"))
      .with_hint("opt in to view it on the website first, then log in here"),
    "gold_only" => denied("this community is for Reddit Premium members only"),
    "banned" => Error::not_found("this community was banned"),
    _ => Error::upstream(format!(
      "Reddit refused the request: {reason} (HTTP {status})"
    )),
  })
}

/// The `{"json": {"errors": [[CODE, message, field], ...]}}` envelope of writes.
pub(super) fn api_errors(v: &Value) -> Result<()> {
  let Some(e) = v.list("json.errors").first() else {
    return Ok(());
  };
  let code = e.str("0").unwrap_or_default();
  let text = match e.str("1") {
    Some(message) => format!("{message} ({code})"),
    None => code.clone(),
  };
  Err(match code.as_str() {
    "RATELIMIT" => Error::new(ErrorCode::RateLimited, text),
    "USER_REQUIRED" | "INVALID_USER" => Error::auth(text),
    "BAD_CAPTCHA" => Error::new(ErrorCode::VerificationRequired, text),
    "SUBREDDIT_NOEXIST" | "NO_THING_ID" | "DELETED_COMMENT" | "DELETED_LINK"
    | "USER_DOESNT_EXIST" => Error::not_found(text),
    "THREAD_LOCKED"
    | "TOO_OLD"
    | "SUBREDDIT_NOTALLOWED"
    | "SUBREDDIT_NO_ACCESS"
    | "BANNED_FROM_SUBREDDIT"
    | "NOT_AUTHOR"
    | "NOT_ALLOWED" => Error::new(ErrorCode::PermissionDenied, text),
    _ => Error::input(text),
  })
}
