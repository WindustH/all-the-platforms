//! `h5st` 5.3, the signature JD's web pages put on `api.m.jd.com` calls.
//!
//! Ported by reading `js_security_v3` (`ParamsSign`, file version 5.3.5)
//! and checked against signatures that library made (catbus's recorded
//! runs); nothing of it runs here. Per signing app id the library keeps a
//! 16-character fingerprint for a year and asks `cactus.jd.com/request_algo`
//! for a token (`tk03…`, a day) and the function that turns token,
//! fingerprint, time and app id into the HMAC key. Without one it makes up
//! a local token (`tk06…`) and derives the key itself; JD's business APIs
//! refuse those. The h5st is ten `;`-separated parts: time, fingerprint,
//! app id, token, HMAC of the signed parameters, version, milliseconds,
//! the encoded environment, HMAC of a fixed parameter list, and the
//! encoded names of the signed parameters.

pub mod crypto;
mod env;
mod token;

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use atp_core::{Ctx, Error, Result, ValueExt, json};
use rand::Rng;
use serde::{Deserialize, Serialize};

use self::crypto::Hash;
use self::env::{Context, FILE_VERSION};
use self::token::{KeyInputs, Recipe};
use crate::parse::unescape;

const VERSION: &str = "5.3";
const ALGO_URL: &str = "https://cactus.jd.com/request_algo";
/// The server says a day; renew an hour early.
const TOKEN_LIFE: Duration = Duration::from_secs(23 * 3600);
const YEAR: Duration = Duration::from_secs(365 * 24 * 3600);
/// The library's clock runs three seconds ahead.
const SKEW_MS: i64 = 3000;
/// Appended to the time the key is derived from.
const TIME_SUFFIX: &str = "52";
/// What the second HMAC covers.
const NAME_LIST: &str = "appid:appid&functionId:functionid";

/// The page a call is made from.
pub struct Page<'a> {
  pub url: &'a str,
  pub origin: &'a str,
}

/// A signed request's extra parameters.
pub struct Signed {
  pub h5st: String,
  /// Why a local token was used, when it was.
  pub local: Option<String>,
}

/// A token and its key recipe; `algo` is empty for a local token.
#[derive(Clone, Serialize, Deserialize)]
struct Grant {
  fp: String,
  token: String,
  algo: String,
}

/// Tokens per signing app id, kept for the run and in the cache.
#[derive(Default)]
pub struct Signer {
  grants: RefCell<HashMap<String, Grant>>,
}

impl Signer {
  /// The h5st of `params` (their values as signed: the body as its
  /// SHA-256) for the signing app id `app_id`.
  pub async fn sign(
    &self,
    ctx: &Ctx,
    app_id: &str,
    page: &Page<'_>,
    params: &[(&str, String)],
  ) -> Result<Signed> {
    let fp = stored(ctx, &format!("h5st-fp-{app_id}"), token::fingerprint);
    let canvas = stored(ctx, "h5st-canvas", random_md5);
    let user_agent = ctx
      .http
      .default_header("user-agent")
      .unwrap_or_else(|| atp_core::http::USER_AGENT.to_owned());
    let env = Context {
      user_agent: &user_agent,
      canvas: &canvas,
      pins: ["pwdt_id", "pin", "pt_pin"].map(|c| ctx.http.cookie(c).map(|v| unescape(&v))),
      cookies: ctx.http.cookie_header(),
      page: page.url,
      origin: page.origin,
    };
    let now = jiff::Timestamp::now().as_millisecond();
    let (grant, local) = match self.grant(ctx, app_id, &fp, &env).await {
      Ok(grant) => (grant, None),
      Err(e) => {
        let token = token::local_token(&fp, now);
        let algo = String::new();
        let fp = fp.clone();
        (Grant { fp, token, algo }, Some(e.message))
      }
    };
    let environment = serde_json::to_string_pretty(&env.short(&fp, now))?;
    let h5st = assemble(&grant, app_id, now, &environment, params)?;
    Ok(Signed { h5st, local })
  }

  /// The server token for `app_id`: from memory, the cache, or a new one.
  async fn grant(&self, ctx: &Ctx, app_id: &str, fp: &str, env: &Context<'_>) -> Result<Grant> {
    if let Some(grant) = self.grants.borrow().get(app_id) {
      return Ok(grant.clone());
    }
    let key = format!("h5st-token-{app_id}");
    let cached = ctx
      .store
      .cache_get::<Grant>(&key, TOKEN_LIFE)
      .filter(|g| g.fp == fp);
    let grant = match cached {
      Some(grant) => grant,
      None => {
        let grant = request(ctx, app_id, fp, env).await?;
        ctx.store.cache_put(&key, &grant);
        grant
      }
    };
    self
      .grants
      .borrow_mut()
      .insert(app_id.to_owned(), grant.clone());
    Ok(grant)
  }
}

/// The ten parts, the HMACs keyed by what the grant's recipe derives.
fn assemble(
  grant: &Grant,
  app_id: &str,
  now: i64,
  environment: &str,
  params: &[(&str, String)],
) -> Result<String> {
  let time = clock(now + SKEW_MS);
  let inputs = KeyInputs {
    token: &grant.token,
    fp: &grant.fp,
    time: &format!("{time}{TIME_SUFFIX}"),
    app_id,
  };
  let key = if grant.algo.is_empty() {
    token::local_key(&grant.token, &grant.fp, inputs.time, app_id)?
  } else {
    Recipe::parse(&grant.algo)?.key(&inputs)
  };
  let mut params: Vec<&(&str, String)> = params.iter().collect();
  params.sort_by_key(|(name, _)| *name);
  let joined: Vec<String> = params.iter().map(|(k, v)| format!("{k}:{v}")).collect();
  let names: Vec<&str> = params.iter().map(|(k, _)| *k).collect();
  let parts = [
    time,
    grant.fp.clone(),
    app_id.to_owned(),
    grant.token.clone(),
    Hash::Sha256.hmac(&joined.join("&"), &key),
    VERSION.to_owned(),
    now.to_string(),
    crypto::encode(environment.as_bytes()),
    Hash::Sha256.hmac(NAME_LIST, &key),
    crypto::encode(names.join(",").as_bytes()),
  ];
  Ok(parts.join(";"))
}

/// Ask `request_algo` for a token, sending the environment as the
/// library does (without cookies).
async fn request(ctx: &Ctx, app_id: &str, fp: &str, env: &Context<'_>) -> Result<Grant> {
  let now = jiff::Timestamp::now().as_millisecond();
  let expand = serde_json::to_string_pretty(&env.full(app_id, fp))?;
  let body = json!({
    "version": VERSION,
    "fp": fp,
    "appId": app_id,
    "timestamp": now,
    "platform": "web",
    "expandParams": crypto::encode(expand.as_bytes()),
    "fv": FILE_VERSION,
    "localTk": token::local_token(fp, now),
  });
  let v = ctx
    .http
    .post(ALGO_URL)
    .no_cookies()
    .header("accept", "application/json")
    .header("content-type", "application/json")
    .header("origin", env.origin)
    .header("referer", format!("{}/", env.origin))
    .header("sec-fetch-site", "same-site")
    .json_text(body.to_string())
    .value()
    .await?;
  let result = v.at("data.result");
  let (Some(200), Some(token), Some(algo)) =
    (v.i64("status"), result.str("tk"), result.str("algo"))
  else {
    return Err(Error::upstream(format!(
      "JD's request_algo gave no token: {}",
      atp_core::text::truncate(&v.to_string(), 160)
    )));
  };
  Recipe::parse(&algo)?;
  Ok(Grant {
    fp: fp.to_owned(),
    token,
    algo,
  })
}

/// A value kept in the cache for a year, made on first use.
fn stored(ctx: &Ctx, key: &str, make: impl FnOnce() -> String) -> String {
  if let Some(value) = ctx.store.cache_get::<String>(key, YEAR) {
    return value;
  }
  let value = make();
  ctx.store.cache_put(key, &value);
  value
}

fn random_md5() -> String {
  let bytes: [u8; 16] = rand::rng().random();
  crypto::plain_hex(&bytes)
}

/// `yyyyMMddHHmmssSSS` in Beijing time.
fn clock(ms: i64) -> String {
  let time = jiff::Timestamp::from_millisecond(ms).unwrap_or_else(|_| jiff::Timestamp::now());
  let zoned = time.to_zoned(jiff::tz::TimeZone::fixed(jiff::tz::offset(8)));
  format!(
    "{}{:03}",
    zoned.strftime("%Y%m%d%H%M%S"),
    zoned.millisecond()
  )
}
