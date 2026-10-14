//! Tencent Map WebService transport (`https://apis.map.qq.com`). Every
//! request carries the key and, when an SK is set, `sig`. Credentials come
//! from `TENCENT_MAP_KEY` / `TENCENT_MAP_SK` (they win) or `login --token
//! KEY[:SK]`; `status` codes map onto atp errors.

use atp_core::credentials;
use atp_core::geo::System;
use atp_core::http::Req;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

use crate::sign;

pub const API: &str = "https://apis.map.qq.com";
const KEY: &str = "key";
const SK: &str = "sk";
const KEYS: &str = "with a key that has 「WebServiceAPI」 enabled, from https://lbs.qq.com/dev/console/application/mine";

/// A key and its optional signing secret.
#[derive(Clone)]
pub struct Creds {
  pub key: String,
  pub sk: Option<String>,
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<Creds>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env = credentials::env("TENCENT_MAP_KEY").map(|key| Creds {
      key,
      sk: credentials::env("TENCENT_MAP_SK"),
    });
    Self { ctx, env }
  }

  pub fn creds(&self) -> Option<Creds> {
    if let Some(c) = &self.env {
      return Some(c.clone());
    }
    let key = self.ctx.extra(KEY).filter(|k| !k.is_empty())?;
    let sk = self.ctx.extra(SK).filter(|s| !s.is_empty());
    Some(Creds { key, sk })
  }

  pub fn logged_in(&self) -> bool {
    self.creds().is_some()
  }

  pub fn require(&self) -> Result<Creds> {
    self
      .creds()
      .ok_or_else(|| Error::auth("no Tencent Map key").with_hint(self.login_hint()))
  }

  /// How to log in, and where keys come from.
  fn login_hint(&self) -> String {
    format!("{} {KEYS}", self.ctx.login_hint())
  }

  /// Keep `KEY[:SK]` in the session.
  pub fn save(&self, token: &str) -> Result<()> {
    let fields = credentials::expect(token, 1, 2, "KEY[:SK]")?;
    let sk = fields.get(1).map(String::as_str).unwrap_or_default();
    credentials::save(&self.ctx, &[(KEY, &fields[0]), (SK, sk)]);
    Ok(())
  }

  /// The system of the points users give and see (`--coord`).
  pub fn system(&self) -> System {
    self.ctx.coord.unwrap_or(System::Gcj02)
  }

  /// GET a JSON endpoint; empty parameters are left out.
  pub async fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
    let v = self.request(path, params)?.send().await?.check()?.value()?;
    check(v, &self.login_hint())
  }

  /// GET an endpoint that answers with an image (static maps); errors
  /// come back as JSON.
  pub async fn image(&self, path: &str, params: &[(&str, String)]) -> Result<Vec<u8>> {
    let resp = self.request(path, params)?.send().await?;
    let image = resp
      .header("content-type")
      .is_some_and(|t| t.starts_with("image/"));
    if image {
      return Ok(resp.check()?.body);
    }
    check(resp.check()?.value()?, &self.login_hint())?;
    Err(Error::upstream("Tencent answered without an image"))
  }

  fn request(&self, path: &str, params: &[(&str, String)]) -> Result<Req<'_>> {
    let creds = self.require()?;
    let mut pairs: Vec<(String, String)> = params
      .iter()
      .filter(|(_, v)| !v.is_empty())
      .map(|(k, v)| ((*k).to_owned(), v.clone()))
      .collect();
    pairs.push(("key".into(), creds.key));
    let special = pairs.iter().any(|(_, v)| v.contains(['&', '#', '%', '+']));
    let mut query = sign::query(&pairs);
    if let Some(sk) = &creds.sk {
      query = format!("{query}&sig={}", sign::sig(path, &pairs, sk));
    }
    let mut req = self
      .ctx
      .http
      .get(format!("{API}{path}?{query}"))
      .no_cookies();
    if special {
      req = req.header("x-legacy-url-decode", "no");
    }
    Ok(req)
  }
}

fn check(v: Value, login: &str) -> Result<Value> {
  match v.i64("status") {
    Some(0) => Ok(v),
    Some(code) => Err(error(code, &v, login)),
    None => Err(Error::upstream("Tencent answered without a status")),
  }
}

/// The atp error for a failed answer, by its `status`.
fn error(code: i64, v: &Value, login: &str) -> Error {
  let message = format!(
    "Tencent Map: {} ({code})",
    v.str("message").unwrap_or_default()
  );
  match code {
    190 | 301 | 311 => Error::auth(message).with_hint(login),
    120 | 121 => Error::new(ErrorCode::RateLimited, message)
      .with_hint("wait a moment, or raise the key's quota in the Tencent console"),
    111 => Error::new(ErrorCode::SignatureError, message).with_hint(
      "check TENCENT_MAP_SK or the SK after `:` in `login --token` (the key's SN check secret)",
    ),
    110 | 112 | 408 => Error::new(ErrorCode::PermissionDenied, message).with_hint(
      "this machine is not among the key's allowed IPs or domains (console → key settings)",
    ),
    113 | 199 => Error::new(ErrorCode::PermissionDenied, message)
      .with_hint("enable 「WebServiceAPI」 and give the key quota for this service in the console"),
    347 | 375 | 377..=379 | 382 | 384 | 387 | 393 | 327..=329 | 335 | 344 => {
      Error::not_found(message)
    }
    300..=400 | 402 => Error::input(message),
    500..=531 => Error::network(message),
    _ => Error::upstream(message),
  }
}
