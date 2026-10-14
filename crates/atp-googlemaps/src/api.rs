//! Google Maps Platform transport. The newer APIs (Places, Routes) take
//! JSON with the key in `X-Goog-Api-Key` and a field mask in
//! `X-Goog-FieldMask`, and fail with Google's `{error: {status, details}}`
//! envelope; the web services (Geocoding, Time Zone, Elevation, Static
//! Maps) take `key=` and answer with a `status`. The key comes from
//! `GOOGLE_MAPS_API_KEY` (it wins) or `login --token KEY`.

use atp_core::credentials;
use atp_core::geo::System;
use atp_core::http::{Req, Resp};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

pub const PLACES: &str = "https://places.googleapis.com/v1";
pub const ROUTES: &str = "https://routes.googleapis.com";
pub const MAPS: &str = "https://maps.googleapis.com/maps/api";
const KEY: &str = "key";
const ENV_KEY: &str = "GOOGLE_MAPS_API_KEY";
const ENV_LANGUAGE: &str = "GOOGLE_MAPS_LANGUAGE";
const CONSOLE: &str = "https://console.cloud.google.com/google/maps-apis/credentials";

pub struct Api {
  pub ctx: Ctx,
  env: Option<String>,
  /// `GOOGLE_MAPS_LANGUAGE`: the language of names and addresses.
  pub language: Option<String>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      env: credentials::env(ENV_KEY),
      language: credentials::env(ENV_LANGUAGE),
    }
  }

  pub fn key(&self) -> Option<String> {
    self
      .env
      .clone()
      .or_else(|| self.ctx.extra(KEY).filter(|k| !k.is_empty()))
  }

  pub fn logged_in(&self) -> bool {
    self.key().is_some()
  }

  pub fn require(&self) -> Result<String> {
    self.key().ok_or_else(|| {
      Error::auth("no Google Maps API key").with_hint(format!(
        "{}; create a key at {CONSOLE}",
        self.ctx.login_hint()
      ))
    })
  }

  /// Keep the key in the session.
  pub fn save(&self, token: &str) -> Result<()> {
    let fields = credentials::expect(token, 1, 1, "KEY")?;
    credentials::save(&self.ctx, &[(KEY, &fields[0])]);
    Ok(())
  }

  /// The system of the points users give and see (`--coord`).
  pub fn system(&self) -> System {
    self.ctx.coord.unwrap_or(System::Wgs84)
  }

  /// POST JSON to a newer API, asking only for `fields`.
  pub async fn post(&self, url: &str, fields: &str, body: &Value) -> Result<Value> {
    let req = self.ctx.http.post(url).json(body);
    self.send(req, fields).await
  }

  /// GET a newer API's resource, asking only for `fields`.
  pub async fn fetch(&self, url: &str, fields: &str, query: &[(&str, String)]) -> Result<Value> {
    let req = self.ctx.http.get(url).queries(query.iter().cloned());
    self.send(req, fields).await
  }

  async fn send(&self, req: Req<'_>, fields: &str) -> Result<Value> {
    let key = self.require()?;
    let mut req = req.no_cookies().header("x-goog-api-key", &key);
    if !fields.is_empty() {
      req = req.header("x-goog-fieldmask", fields);
    }
    let resp = req.send().await?;
    if resp.status.is_success() {
      return resp.value();
    }
    Err(self.envelope_error(&resp))
  }

  /// GET a web service (`/geocode/json` …); `OK` and `ZERO_RESULTS` pass.
  pub async fn service(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
    let resp = self.service_req(path, params)?.send().await?;
    let v = resp.check()?.value()?;
    match v.str("status").as_deref() {
      Some("OK" | "ZERO_RESULTS") => Ok(v),
      status => Err(self.service_error(status.unwrap_or_default(), &v)),
    }
  }

  /// GET a web service that answers with an image (static maps).
  pub async fn image(&self, path: &str, params: &[(&str, String)]) -> Result<Resp> {
    let resp = self.service_req(path, params)?.send().await?;
    let image = resp
      .header("content-type")
      .is_some_and(|t| t.starts_with("image/"));
    if resp.status.is_success() && image {
      return Ok(resp);
    }
    let text = atp_core::text::one_line(&resp.text());
    let message = format!("Google Maps: {}", atp_core::text::truncate(&text, 200));
    Err(match resp.status.as_u16() {
      403 if text.contains("API key is invalid") => self.auth_error(message),
      403 => Error::new(ErrorCode::PermissionDenied, message).with_hint(ENABLE_HINT),
      400 => Error::input(message),
      429 => Error::new(ErrorCode::RateLimited, message),
      _ => Error::upstream(message),
    })
  }

  fn service_req(&self, path: &str, params: &[(&str, String)]) -> Result<Req<'_>> {
    let key = self.require()?;
    let mut pairs: Vec<(String, String)> = params
      .iter()
      .filter(|(_, v)| !v.is_empty())
      .map(|(k, v)| ((*k).to_owned(), v.clone()))
      .collect();
    if let Some(language) = &self.language {
      pairs.push(("language".into(), language.clone()));
    }
    pairs.push(("key".into(), key));
    let url = format!("{MAPS}{path}");
    Ok(self.ctx.http.get(url).no_cookies().queries(pairs))
  }

  fn auth_error(&self, message: String) -> Error {
    Error::auth(message).with_hint(format!(
      "{} with a key from {CONSOLE}",
      self.ctx.login_hint()
    ))
  }

  /// The error of a newer API, by `error.details[].reason` first.
  fn envelope_error(&self, resp: &Resp) -> Error {
    let v: Value = resp.value().unwrap_or_default();
    let e = v.at("error");
    let text = e.str("message").unwrap_or_else(|| resp.text());
    let message = format!("Google Maps: {}", atp_core::text::truncate(&text, 300));
    let reason = e
      .list("details")
      .iter()
      .find_map(|d| d.str("reason"))
      .unwrap_or_default();
    match reason.as_str() {
      "API_KEY_INVALID" => return self.auth_error(message),
      "SERVICE_DISABLED" | "API_KEY_SERVICE_BLOCKED" => {
        return Error::new(ErrorCode::PermissionDenied, message).with_hint(ENABLE_HINT);
      }
      "API_KEY_HTTP_REFERRER_BLOCKED" | "API_KEY_IP_ADDRESS_BLOCKED" => {
        return Error::new(ErrorCode::PermissionDenied, message).with_hint(RESTRICTION_HINT);
      }
      "BILLING_DISABLED" => {
        return Error::new(ErrorCode::PermissionDenied, message).with_hint(BILLING_HINT);
      }
      "RATE_LIMIT_EXCEEDED" | "RESOURCE_QUOTA_EXCEEDED" => {
        return Error::new(ErrorCode::RateLimited, message).with_hint(QUOTA_HINT);
      }
      _ => {}
    }
    match (resp.status.as_u16(), e.str("status").as_deref()) {
      (403, _) if text.contains("API key") || text.contains("unregistered callers") => {
        self.auth_error(message)
      }
      (429, _) | (_, Some("RESOURCE_EXHAUSTED")) => {
        Error::new(ErrorCode::RateLimited, message).with_hint(QUOTA_HINT)
      }
      (403, _) => Error::new(ErrorCode::PermissionDenied, message),
      (404, _) => Error::not_found(message),
      (400, _) => Error::input(message),
      _ => Error::upstream(message),
    }
  }

  /// The error of a web service's `status`.
  fn service_error(&self, status: &str, v: &Value) -> Error {
    let detail = v
      .first_str(&["error_message", "errorMessage"])
      .unwrap_or_default();
    let message = format!("Google Maps: {status} {detail}")
      .trim_end()
      .to_owned();
    match status {
      "REQUEST_DENIED" if detail.contains("not authorized") => {
        Error::new(ErrorCode::PermissionDenied, message).with_hint(ENABLE_HINT)
      }
      "REQUEST_DENIED" if detail.contains("Billing") => {
        Error::new(ErrorCode::PermissionDenied, message).with_hint(BILLING_HINT)
      }
      "REQUEST_DENIED" | "OVER_DAILY_LIMIT" => self.auth_error(message),
      "OVER_QUERY_LIMIT" => Error::new(ErrorCode::RateLimited, message).with_hint(QUOTA_HINT),
      "INVALID_REQUEST" => Error::input(message),
      "DATA_NOT_AVAILABLE" | "NOT_FOUND" => Error::not_found(message),
      "UNKNOWN_ERROR" => Error::network(message),
      _ => Error::upstream(message),
    }
  }
}

const ENABLE_HINT: &str = "enable this API for the key's project (Google Cloud console → APIs & \
   Services → Library), and allow it in the key's API restrictions";
const RESTRICTION_HINT: &str =
  "the key's application restrictions (referrers, IP addresses) refuse this machine";
const BILLING_HINT: &str = "Google Maps needs billing on the key's project: \
   https://console.cloud.google.com/project/_/billing/enable";
const QUOTA_HINT: &str = "wait a moment, or raise the quota in the Google Cloud console";
