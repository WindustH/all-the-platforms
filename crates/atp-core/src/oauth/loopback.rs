//! Setting up the local end of a login that comes back through the
//! browser: a random `state` (or PKCE verifier), the port the listener
//! takes, the listener on the loopback interface, and the wait for the
//! browser to come back to it.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::time::Duration;

use rand::Rng;
use tokio::net::TcpListener;

use super::{Via, redirect};
use crate::credentials;
use crate::error::{Error, Result};

/// How long a login waits for the browser to come back.
const CONSENT_WAIT: Duration = Duration::from_secs(300);
const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// `len` random letters and digits (`state`, PKCE verifiers).
pub fn random(len: usize) -> String {
  let mut rng = rand::rng();
  (0..len)
    .map(|_| CHARS[rng.random_range(0..CHARS.len())] as char)
    .collect()
}

/// The port the listener takes: the environment variable `env` when it is
/// set, else `default`.
pub fn port(env: &str, default: u16) -> Result<u16> {
  match credentials::env(env) {
    None => Ok(default),
    Some(text) => text
      .parse()
      .ok()
      .filter(|p| *p > 0)
      .ok_or_else(|| Error::input(format!("{env} is not a port number: {text}"))),
  }
}

/// A listener on `127.0.0.1:port`; a busy port fails with a hint naming
/// `env`, the variable that picks another one.
pub async fn listen(port: u16, env: &str) -> Result<TcpListener> {
  TcpListener::bind((Ipv4Addr::LOCALHOST, port))
    .await
    .map_err(|e| {
      Error::input(format!(
        "cannot listen on 127.0.0.1:{port} for the redirect: {e}"
      ))
      .with_hint(format!("free the port, or set {env} to another one"))
    })
}

/// [`redirect`], given five minutes at most.
pub async fn wait(
  listener: &TcpListener,
  path: &str,
  state: &str,
  via: Via,
  site: &str,
) -> Result<BTreeMap<String, String>> {
  let back = redirect(listener, path, state, via, site);
  tokio::time::timeout(CONSENT_WAIT, back)
    .await
    .map_err(|_| Error::auth("timed out waiting for the browser to come back; run login again"))?
}
