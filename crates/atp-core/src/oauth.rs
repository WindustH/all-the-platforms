//! OAuth 2.0 for a command line: the device-code login (RFC 8628), where
//! the user confirms a code or scans a QR code on another device while atp
//! polls; the consent page of a login that comes back through the browser
//! ([`open_consent`]); and tokens kept with the session.
//!
//! The tokens live in the session extras next to the cookies, so one
//! session can carry both (a drive's web cookies and its open-platform
//! token). Some servers hand out refresh tokens that work once (Baidu): a
//! new pair is written to the session file the moment it arrives, before
//! anything else can fail.
//!
//! The platform makes the requests (endpoints, client ids and secrets are
//! its own) and hands the JSON to [`DeviceCode::parse`], [`Poll::parse`]
//! and [`Tokens::parse`], which read the standard fields.

use std::time::{Duration, Instant};

use jiff::Timestamp;
use serde_json::Value;

use crate::ctx::Ctx;
use crate::error::{Error, ErrorCode, Result};
use crate::json::ValueExt;
use crate::output::note;
use crate::qr;

/// Session extras the tokens are kept under.
pub const ACCESS_TOKEN: &str = "oauth_access_token";
pub const REFRESH_TOKEN: &str = "oauth_refresh_token";
/// Unix seconds.
pub const EXPIRES_AT: &str = "oauth_expires_at";

/// Refresh this long before the access token expires.
const MARGIN_SECS: i64 = 600;
/// What RFC 8628 says to poll at when the server does not say.
const DEFAULT_INTERVAL: Duration = Duration::from_secs(5);
/// How much `slow_down` adds to the interval.
const SLOW_DOWN: Duration = Duration::from_secs(5);

/// A started device login.
#[derive(Debug, Clone)]
pub struct DeviceCode {
  pub device_code: String,
  /// What the user types on the verification page.
  pub user_code: String,
  /// The page to open and type `user_code` into.
  pub verification_url: String,
  /// A link that carries the code already; shown as the QR code.
  pub complete_url: Option<String>,
  /// The least time between two polls.
  pub interval: Duration,
  pub expires_in: Duration,
}

impl DeviceCode {
  /// The standard response: `device_code`, `user_code`,
  /// `verification_uri` (or `verification_url`), `verification_uri_complete`,
  /// `interval` and `expires_in` (seconds).
  pub fn parse(v: &Value) -> Result<Self> {
    fail_on_error(v)?;
    let (Some(device_code), Some(user_code)) = (v.str("device_code"), v.str("user_code")) else {
      return Err(Error::upstream("the device-code answer had no code"));
    };
    let verification_url = v
      .first_str(&["verification_uri", "verification_url"])
      .unwrap_or_default();
    let secs = |key: &str, default: u64| v.u64(key).filter(|s| *s > 0).unwrap_or(default);
    Ok(Self {
      device_code,
      user_code,
      verification_url,
      complete_url: v.str("verification_uri_complete"),
      interval: Duration::from_secs(secs("interval", DEFAULT_INTERVAL.as_secs())),
      expires_in: Duration::from_secs(secs("expires_in", 300)),
    })
  }
}

/// An access token, the refresh token that renews it, and when it expires.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tokens {
  pub access: String,
  pub refresh: Option<String>,
  /// Unix seconds.
  pub expires_at: Option<i64>,
}

impl Tokens {
  /// The standard token answer (`access_token`, `refresh_token`,
  /// `expires_in`), or its error (`error`, `error_description`).
  pub fn parse(v: &Value) -> Result<Self> {
    fail_on_error(v)?;
    let access = v
      .str("access_token")
      .filter(|t| !t.is_empty())
      .ok_or_else(|| Error::upstream("the token answer had no access_token"))?;
    let now = Timestamp::now().as_second();
    Ok(Self {
      access,
      refresh: v.str("refresh_token").filter(|t| !t.is_empty()),
      expires_at: v.i64("expires_in").map(|secs| now + secs),
    })
  }

  /// The tokens saved with the session.
  pub fn saved(ctx: &Ctx) -> Option<Self> {
    Some(Self {
      access: ctx.extra(ACCESS_TOKEN).filter(|t| !t.is_empty())?,
      refresh: ctx.extra(REFRESH_TOKEN).filter(|t| !t.is_empty()),
      expires_at: ctx.extra(EXPIRES_AT).and_then(|t| t.parse().ok()),
    })
  }

  /// Whether the access token still has a while to live (or no known end).
  pub fn fresh(&self) -> bool {
    self
      .expires_at
      .is_none_or(|at| at - MARGIN_SECS > Timestamp::now().as_second())
  }

  /// Keep the tokens with the session, and write the session file at once.
  pub fn save(&self, ctx: &Ctx) -> Result<()> {
    let expires = self.expires_at.map(|t| t.to_string()).unwrap_or_default();
    let pairs = [
      (ACCESS_TOKEN, self.access.clone()),
      (REFRESH_TOKEN, self.refresh.clone().unwrap_or_default()),
      (EXPIRES_AT, expires),
    ];
    for (key, value) in &pairs {
      ctx.set_extra(key, value);
    }
    ctx.store.update_session(|session| {
      for (key, value) in pairs {
        session.extra.insert(key.to_owned(), value);
      }
    })
  }
}

/// What one poll of the token endpoint found.
#[derive(Debug, Clone)]
pub enum Poll {
  /// Not confirmed yet.
  Pending,
  /// Polling too fast: wait longer between polls.
  SlowDown,
  Done(Tokens),
}

impl Poll {
  /// A token answer during a device login: `authorization_pending` and
  /// `slow_down` keep it going; `expired_token`, `access_denied` and other
  /// errors end it.
  pub fn parse(v: &Value) -> Result<Self> {
    match v.str("error").as_deref() {
      Some("authorization_pending") => Ok(Self::Pending),
      Some("slow_down") => Ok(Self::SlowDown),
      Some("expired_token") => Err(Error::auth("the code expired; run login again")),
      Some("access_denied") => Err(Error::auth("the login was refused on the other device")),
      _ => Tokens::parse(v).map(Self::Done),
    }
  }
}

/// Show `code` (its page, the code to type, a QR code of the link) and
/// `poll` the token endpoint at the interval the server asked for until
/// the user confirms. The tokens come back unsaved: [`Tokens::save`].
pub async fn device_login(
  ctx: &Ctx,
  code: &DeviceCode,
  mut poll: impl AsyncFnMut() -> Result<Poll>,
) -> Result<Tokens> {
  let link = code
    .complete_url
    .clone()
    .unwrap_or_else(|| code.verification_url.clone());
  let image = qr::show(&link, ctx.store.cache_dir())?;
  note(&format!(
    "Open {} and enter the code {}, or scan the QR code (image: {}).",
    code.verification_url,
    code.user_code,
    image.display()
  ));
  let started = Instant::now();
  let mut interval = code.interval;
  while started.elapsed() < code.expires_in {
    tokio::time::sleep(interval).await;
    match poll().await? {
      Poll::Pending => {}
      Poll::SlowDown => interval += SLOW_DOWN,
      Poll::Done(tokens) => return Ok(tokens),
    }
  }
  Err(Error::auth(
    "the code expired before it was confirmed; run login again",
  ))
}

/// Open the consent page of a login that comes back through the browser
/// (an authorization code at a local address) in the default browser, and
/// print its link for when none opened.
pub fn open_consent(url: &str) {
  let lead = if crate::verify::open_default(url) {
    "Opened the consent page in your browser"
  } else {
    "Open the consent page in a browser"
  };
  note(&format!("{lead}; allow access there. The page:\n{url}"));
}

/// A usable access token: the saved one while it is fresh, else the one
/// `refresh(refresh_token)` gets, saved (with the new refresh token) before
/// it is returned. Fails with `not_authenticated` when nothing is saved.
pub async fn access_token(
  ctx: &Ctx,
  refresh: impl AsyncFnOnce(&str) -> Result<Tokens>,
) -> Result<String> {
  let Some(saved) = Tokens::saved(ctx) else {
    return Err(Error::auth("no access token saved").with_hint(ctx.login_hint()));
  };
  if saved.fresh() {
    return Ok(saved.access);
  }
  let Some(token) = saved.refresh.as_deref() else {
    return Err(Error::auth("the access token expired").with_hint(ctx.login_hint()));
  };
  let mut renewed = refresh(token).await.map_err(|e| expired(e, ctx))?;
  // Servers that keep the refresh token answer without one.
  if renewed.refresh.is_none() {
    renewed.refresh = saved.refresh;
  }
  renewed.save(ctx)?;
  Ok(renewed.access)
}

/// A refused refresh means logging in again.
fn expired(e: Error, ctx: &Ctx) -> Error {
  match e.code {
    ErrorCode::NotAuthenticated | ErrorCode::InvalidInput => {
      Error::auth(format!("the token could not be renewed: {}", e.message))
        .with_hint(ctx.login_hint())
    }
    _ => e,
  }
}

/// An RFC 6749 error answer as an error.
fn fail_on_error(v: &Value) -> Result<()> {
  let Some(error) = v.str("error") else {
    return Ok(());
  };
  let detail = v.str("error_description").unwrap_or_default();
  let message = format!("OAuth error {error}: {detail}");
  Err(match error.as_str() {
    "invalid_grant" | "invalid_client" | "unauthorized_client" => Error::auth(message),
    "invalid_request" | "invalid_scope" => Error::input(message),
    _ => Error::upstream(message),
  })
}
