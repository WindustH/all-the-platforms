//! Logging in with an app password: the handle leads to the account's PDS
//! (its DID document), where `com.atproto.server.createSession` hands
//! out an access JWT and a refresh JWT. Both are kept with the session,
//! and the access JWT is renewed with `refreshSession` before it expires.

use atp_core::http::Method;
use atp_core::oauth::Tokens;
use atp_core::{Error, Result, Value, ValueExt, credentials, json};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use crate::api::{self, Api, DID_KEY, HANDLE_KEY, PDS_KEY};
use crate::identity;

/// Where accounts are created on bsky.social; it finds an email's PDS.
const ENTRYWAY: &str = "https://bsky.social";
const FORMS: &str = "HANDLE|APP_PASSWORD[|PDS_URL]";

/// `HANDLE|APP_PASSWORD`, or `HANDLE|APP_PASSWORD|PDS_URL` to name the
/// PDS outright. `HANDLE` may be the account's DID or email.
pub async fn token(api: &Api, raw: &str) -> Result<()> {
  let fields: Vec<&str> = raw.trim().splitn(3, '|').map(str::trim).collect();
  let [identifier, password, rest @ ..] = fields.as_slice() else {
    return Err(Error::input(format!("expected `login --token '{FORMS}'`")));
  };
  let identifier = identifier.trim_start_matches('@');
  if identifier.is_empty() || password.is_empty() {
    return Err(Error::input(format!("expected `login --token '{FORMS}'`")));
  }
  let pds = match rest.first() {
    Some(url) => format!("https://{}", credentials::host(url)),
    None if identifier.contains('@') => ENTRYWAY.to_owned(),
    None => identity::pds(api, identifier).await?,
  };
  let v = create_session(api, &pds, identifier, password).await?;
  let pds = identity::pds_in(v.at("didDoc")).unwrap_or(pds);
  let did = v
    .str("did")
    .ok_or_else(|| Error::upstream("the session answer had no DID"))?;
  credentials::save(
    &api.ctx,
    &[
      (PDS_KEY, &pds),
      (DID_KEY, &did),
      (HANDLE_KEY, &v.str("handle").unwrap_or_default()),
    ],
  );
  tokens(&v)?.save(&api.ctx)
}

async fn create_session(api: &Api, pds: &str, identifier: &str, password: &str) -> Result<Value> {
  let body = json!({"identifier": identifier, "password": password});
  let resp = api
    .send(
      Method::POST,
      pds,
      "com.atproto.server.createSession",
      Vec::new(),
      Some(body),
      None,
    )
    .await?;
  api::check(&resp).map_err(|e| {
    if e.message.contains("AuthFactorTokenRequired") {
      e.with_hint(
        "the account asks for an email code: log in with an app password instead \
         (Settings → Privacy and security → App passwords)",
      )
    } else if e.code == atp_core::ErrorCode::NotAuthenticated || e.message.contains("Invalid") {
      Error::auth(format!("{pds} refused the login: {}", e.message))
        .with_hint("check the handle and the app password")
    } else {
      e
    }
  })
}

/// Renew the session with the refresh JWT (`refreshSession`).
pub async fn refresh(api: &Api, refresh: &str) -> Result<Tokens> {
  let pds = api.pds().unwrap_or_default();
  let resp = api
    .send(
      Method::POST,
      &pds,
      "com.atproto.server.refreshSession",
      Vec::new(),
      None,
      Some(refresh),
    )
    .await?;
  tokens(&api::check(&resp)?)
}

/// `accessJwt` and `refreshJwt`, with the access JWT's expiry.
fn tokens(v: &Value) -> Result<Tokens> {
  let access = v
    .str("accessJwt")
    .ok_or_else(|| Error::upstream("the session answer had no access JWT"))?;
  Ok(Tokens {
    expires_at: expiry(&access),
    access,
    refresh: v.str("refreshJwt"),
  })
}

/// The `exp` claim of a JWT (Unix seconds).
fn expiry(jwt: &str) -> Option<i64> {
  let payload = jwt.split('.').nth(1)?;
  let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
  serde_json::from_slice::<Value>(&bytes).ok()?.i64("exp")
}
