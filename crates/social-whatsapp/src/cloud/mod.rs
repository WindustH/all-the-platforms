//! The WhatsApp Business Platform Cloud API (`graph.facebook.com`), with a
//! system-user access token and a business phone number id.
//!
//! The Cloud API only sends: incoming messages reach a webhook, so it has no
//! chat list, no history and no `listen`. What it does have: sending text,
//! media and templates, reactions, read receipts, media download, the
//! business profile, phone numbers, templates and messaging analytics.

mod business;
mod insights;
mod messages;

use social_core::credentials;
use social_core::http::Method;
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

/// Session extras of `login --token TOKEN:PHONE_NUMBER_ID[:WABA_ID]`.
pub const TOKEN: &str = "cloud_token";
pub const PHONE: &str = "cloud_phone";
pub const WABA: &str = "cloud_waba";
const GRAPH: &str = "https://graph.facebook.com";
const DEFAULT_VERSION: &str = "v23.0";

pub struct Cloud {
  token: String,
  phone: String,
  waba: Option<String>,
  version: String,
}

impl Cloud {
  /// Saved credentials, else `WHATSAPP_TOKEN` + `WHATSAPP_PHONE_NUMBER_ID`
  /// (+ `WHATSAPP_BUSINESS_ACCOUNT_ID`).
  pub fn from_ctx(ctx: &Ctx) -> Option<Self> {
    let version =
      credentials::env("WHATSAPP_GRAPH_VERSION").unwrap_or_else(|| DEFAULT_VERSION.into());
    if let Some([token, phone]) = credentials::saved(ctx, &[TOKEN, PHONE]).as_deref() {
      return Some(Self {
        token: token.clone(),
        phone: phone.clone(),
        waba: ctx.extra(WABA).filter(|v| !v.is_empty()),
        version,
      });
    }
    Some(Self {
      token: credentials::env("WHATSAPP_TOKEN")?,
      phone: credentials::env("WHATSAPP_PHONE_NUMBER_ID")?,
      waba: credentials::env("WHATSAPP_BUSINESS_ACCOUNT_ID"),
      version,
    })
  }

  /// `TOKEN:PHONE_NUMBER_ID[:WABA_ID]`.
  pub fn parse_token(s: &str) -> Result<(String, String, Option<String>)> {
    let fields = credentials::fields(s);
    let numeric = |p: &String| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    match fields.as_slice() {
      [token, phone] if numeric(phone) => Ok((token.clone(), phone.clone(), None)),
      [token, phone, waba] if numeric(phone) && numeric(waba) => {
        Ok((token.clone(), phone.clone(), Some(waba.clone())))
      }
      _ => Err(Error::input(
        "expected `ACCESS_TOKEN:PHONE_NUMBER_ID` or `ACCESS_TOKEN:PHONE_NUMBER_ID:WABA_ID` (from Meta's WhatsApp Manager / App Dashboard); personal accounts log in with `social whatsapp login` (QR code)",
      )),
    }
  }

  fn url(&self, path: &str) -> String {
    format!("{GRAPH}/{}/{}", self.version, path.trim_start_matches('/'))
  }

  async fn call(
    &self,
    ctx: &Ctx,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
  ) -> Result<Value> {
    let mut req = ctx
      .http
      .request(method, self.url(path))
      .no_cookies()
      .header("authorization", format!("Bearer {}", self.token))
      .queries(query.iter().map(|(k, v)| (*k, v)));
    if let Some(b) = body {
      req = req.json(b);
    }
    check(req.send().await?.value()?)
  }

  async fn get(&self, ctx: &Ctx, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.call(ctx, Method::GET, path, query, None).await
  }

  async fn post(&self, ctx: &Ctx, path: &str, body: &Value) -> Result<Value> {
    self.call(ctx, Method::POST, path, &[], Some(body)).await
  }

  fn waba(&self) -> Result<&str> {
    self.waba.as_deref().ok_or_else(|| {
      Error::input("this needs the WhatsApp Business Account id").with_hint(
        "log in with `--token TOKEN:PHONE_NUMBER_ID:WABA_ID` or set WHATSAPP_BUSINESS_ACCOUNT_ID",
      )
    })
  }
}

fn day(t: &jiff::Timestamp) -> String {
  t.strftime("%Y-%m-%d").to_string()
}

/// A recipient's number in Cloud API form (digits with country code).
fn recipient(to: &str) -> Result<String> {
  let to = to.split('@').next().unwrap_or(to);
  crate::refs::phone(to).ok_or_else(|| {
    Error::input(format!(
      "not a phone number: {to} (give it with its country code, e.g. +44 7700 900123)"
    ))
  })
}

/// The `wamid.…` part of `TO/WAMID`.
fn message_id(r: &str) -> &str {
  r.rsplit_once('/').map_or(r, |(_, id)| id)
}

/// Graph API errors into the shared error codes.
fn check(v: Value) -> Result<Value> {
  let Some(e) = v.get("error").filter(|e| e.is_object()) else {
    return Ok(v);
  };
  let code = e.i64("code").unwrap_or_default();
  let detail = e
    .str("error_data.details")
    .or_else(|| e.str("message"))
    .unwrap_or_else(|| "unknown error".into());
  let message = format!("Cloud API error {code}: {detail}");
  Err(match code {
    190 => Error::auth(message).with_hint("the access token expired or is invalid; create a system-user token in Meta Business Manager"),
    10 | 200..=299 => Error::new(ErrorCode::PermissionDenied, message)
      .with_hint("the token lacks whatsapp_business_messaging / whatsapp_business_management"),
    4 | 80007 | 130429 | 131056 => Error::new(ErrorCode::RateLimited, message),
    131047 => Error::new(ErrorCode::PermissionDenied, message).with_hint(
      "outside the 24-hour customer service window only templates can be sent: `social whatsapp template TO NAME`",
    ),
    100 | 131008 | 131009 => Error::input(message),
    _ => Error::upstream(message),
  })
}
