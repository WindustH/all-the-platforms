//! Answers: the `result` envelopes, GraphQL errors and slider captchas.

use atp_core::{Error, ErrorCode, Result, Value, ValueExt};

use super::{CAPTCHA_HINT, LOGIN_HINT, RISK_HINT};

/// The `result` envelope of www and creator-center answers.
pub fn check(v: Value) -> Result<Value> {
  if let Some(e) = captcha(&v) {
    return Err(e);
  }
  match v.i64("result") {
    None | Some(1) => Ok(v),
    Some(code) => Err(failure(code, &v)),
  }
}

/// Live answers: `{data: {result, ...}}`; returns `data`.
pub(super) fn check_live(v: Value) -> Result<Value> {
  if let Some(e) = captcha(&v) {
    return Err(e);
  }
  let data = v.at("data");
  match data.i64("result") {
    None | Some(1) => Ok(data.clone()),
    Some(code) => Err(failure(code, data)),
  }
}

pub(super) fn check_graphql(v: Value) -> Result<Value> {
  if let Some(e) = captcha(&v) {
    return Err(e);
  }
  if let Some(err) = v.list("errors").first() {
    let msg = err.str("message").unwrap_or_else(|| "GraphQL error".into());
    if msg.to_lowercase().contains("login") || msg.contains("登录") {
      return Err(Error::auth(format!("Kuaishou: {msg}")).with_hint(LOGIN_HINT));
    }
    return Err(Error::upstream(format!("Kuaishou GraphQL: {msg}")));
  }
  Ok(v.at("data").clone())
}

fn failure(code: i64, v: &Value) -> Error {
  let msg = v
    .first_str(&["error_msg", "message", "msg"])
    .unwrap_or_else(|| "request failed".into());
  match code {
    109 => Error::auth("Kuaishou wants a login for this (result 109)").with_hint(LOGIN_HINT),
    2 => Error::new(
      ErrorCode::RateLimited,
      "Kuaishou refused the request as risky (result 2)",
    )
    .with_hint(RISK_HINT),
    50 => Error::new(
      ErrorCode::SignatureError,
      "Kuaishou rejected the request signature (result 50)",
    ),
    _ => Error::upstream(format!("Kuaishou: {msg} (result {code})")),
  }
}

/// A slider captcha: REST `{data: {result: 400002, url}}`, GraphQL
/// `data.captcha.url`. Its link goes into the message.
fn captcha(v: &Value) -> Option<Error> {
  let rest = [v, v.at("data")]
    .into_iter()
    .find(|d| d.i64("result") == Some(400002));
  let url = match rest {
    Some(d) => d.str("url"),
    None => Some(v.str("data.captcha.url")?),
  };
  let msg = match url {
    Some(url) => format!("Kuaishou asks for a slider captcha: {url}"),
    None => "Kuaishou asks for a slider captcha".to_owned(),
  };
  Some(Error::new(ErrorCode::VerificationRequired, msg).with_hint(CAPTCHA_HINT))
}
