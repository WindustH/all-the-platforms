//! Logging in with the user's own Stack Apps registration:
//!
//! - `KEY`: the app key alone raises the daily quota from 300 requests per
//!   IP to 10,000;
//! - `KEY:ACCESS_TOKEN`: a token got some other way;
//! - `CLIENT_ID:KEY`: the implicit OAuth flow. The consent page
//!   (`stackexchange.com/oauth/dialog`) opens in the browser and comes back
//!   to `http://localhost:PORT/callback` with the token after `#`, which a
//!   one-shot local listener takes.

use std::net::Ipv4Addr;
use std::time::Duration;

use atp_core::oauth::{self, Via};
use atp_core::output::note;
use atp_core::{Error, Result, credentials};
use rand::Rng;
use tokio::net::TcpListener;

use crate::api::{ACCESS_TOKEN, Api, KEY};

const DIALOG_URL: &str = "https://stackexchange.com/oauth/dialog";
/// Reading the inbox, writing (votes, comments, answers), and a token
/// that does not expire.
const SCOPE: &str = "read_inbox,write_access,no_expiry";
pub const DEFAULT_PORT: u16 = 8898;
pub const ENV_PORT: &str = "SE_REDIRECT_PORT";
const CONSENT_WAIT: Duration = Duration::from_secs(300);
const FORMS: &str = "KEY[:ACCESS_TOKEN]' or 'CLIENT_ID:KEY";

pub async fn token(api: &Api, raw: &str) -> Result<()> {
  let fields = credentials::expect(raw, 1, 2, FORMS)?;
  match fields.as_slice() {
    [key] => save(api, key, ""),
    [client, key] if client.bytes().all(|b| b.is_ascii_digit()) => {
      let token = consent(client).await?;
      save(api, key, &token)
    }
    [key, token] => save(api, key, token),
    _ => Err(Error::input(format!("expected `login --token '{FORMS}'`"))),
  }
}

fn save(api: &Api, key: &str, token: &str) -> Result<()> {
  credentials::save(&api.ctx, &[(KEY, key), (ACCESS_TOKEN, token)]);
  Ok(())
}

/// The consent page in the browser; the access token it sends back.
async fn consent(client: &str) -> Result<String> {
  let port = port()?;
  let redirect = format!("http://localhost:{port}/callback");
  let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
    .await
    .map_err(|e| {
      Error::input(format!(
        "cannot listen on 127.0.0.1:{port} for the redirect: {e}"
      ))
      .with_hint(format!("free the port, or set {ENV_PORT} to another one"))
    })?;
  let state = random(24);
  let params = [
    ("client_id", client),
    ("scope", SCOPE),
    ("redirect_uri", redirect.as_str()),
    ("state", state.as_str()),
  ];
  let url =
    url::Url::parse_with_params(DIALOG_URL, params).map_err(|e| Error::internal(e.to_string()))?;
  oauth::open_consent(url.as_str());
  note(
    "The app needs OAuth Domain `localhost` and \"Enable Client Side OAuth Flow\" ticked; \
     if Stack Exchange refuses the redirect, see `atp guide login so`.",
  );
  let back = oauth::redirect(
    &listener,
    "/callback",
    &state,
    Via::Fragment,
    "Stack Exchange",
  );
  let mut params = tokio::time::timeout(CONSENT_WAIT, back)
    .await
    .map_err(|_| {
      Error::auth("timed out waiting for the browser to come back; run login again")
    })??;
  params
    .remove("access_token")
    .filter(|token| !token.is_empty())
    .ok_or_else(|| Error::auth("Stack Exchange came back without an access token"))
}

/// The listening port: `SE_REDIRECT_PORT`, else 8898.
fn port() -> Result<u16> {
  match credentials::env(ENV_PORT) {
    None => Ok(DEFAULT_PORT),
    Some(text) => text
      .parse()
      .ok()
      .filter(|p| *p > 0)
      .ok_or_else(|| Error::input(format!("{ENV_PORT} is not a port number: {text}"))),
  }
}

/// `len` random letters and digits.
fn random(len: usize) -> String {
  const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  let mut rng = rand::rng();
  (0..len)
    .map(|_| CHARS[rng.random_range(0..CHARS.len())] as char)
    .collect()
}
