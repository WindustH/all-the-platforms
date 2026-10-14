//! Logging in with a token of your own Meta app with the Threads use case:
//! a long-lived user token (60 days) as it is, or a short-lived one
//! exchanged for a long-lived one with the app secret
//! (`th_exchange_token`). Meta accepts only HTTPS redirect addresses for
//! the Threads login, so there is no browser login through a local
//! address. Long-lived tokens are renewed with `th_refresh_token`.

use atp_core::{Error, Result, ValueExt};
use atp_meta::Graph;
use atp_meta::graph::{self, THREADS, fields, params};
use jiff::Timestamp;

use crate::api::{Api, REFRESHED_KEY, USER_ID_KEY, USERNAME_KEY};

/// `ACCESS_TOKEN` (long-lived), or `APP_SECRET|SHORT_LIVED_TOKEN`.
pub async fn token(api: &Api, raw: &str) -> Result<()> {
  let parts: Vec<&str> = raw.trim().splitn(2, '|').map(str::trim).collect();
  let token = match parts.as_slice() {
    [secret, short] if !secret.is_empty() && !short.is_empty() => {
      exchange(api, secret, short).await?
    }
    [token] if !token.is_empty() => (*token).to_owned(),
    _ => {
      return Err(Error::input(
        "expected `login --token ACCESS_TOKEN` or `login --token 'APP_SECRET|SHORT_LIVED_TOKEN'`",
      ));
    }
  };
  let v = Graph::threads(&api.ctx, token.clone())
    .get("me", &fields("id,username"))
    .await?;
  let id = v
    .str("id")
    .ok_or_else(|| Error::auth("the token names no Threads account"))?;
  save(api, &token)?;
  api.ctx.set_extra(USER_ID_KEY, &id);
  api
    .ctx
    .set_extra(USERNAME_KEY, &v.str("username").unwrap_or_default());
  Ok(())
}

/// A short-lived token → a long-lived one.
async fn exchange(api: &Api, secret: &str, short: &str) -> Result<String> {
  let query = params(&[
    ("grant_type", "th_exchange_token"),
    ("client_secret", secret),
  ]);
  let v = Graph::threads(&api.ctx, short)
    .get(&format!("{THREADS}/access_token"), &query)
    .await
    .map_err(|e| {
      e.with_hint("check the app secret (App settings → Basic) and that the token is fresh")
    })?;
  v.str("access_token")
    .ok_or_else(|| Error::upstream("the exchange answered without a token"))
}

/// Renew a long-lived token (it must be a day old and not expired); the
/// new one is saved at once.
pub async fn refresh(api: &Api, token: &str) -> Result<String> {
  let query = params(&[("grant_type", "th_refresh_token")]);
  let v = Graph::threads(&api.ctx, token)
    .get(&format!("{THREADS}/refresh_access_token"), &query)
    .await?;
  let renewed = v
    .str("access_token")
    .ok_or_else(|| Error::upstream("the renewal answered without a token"))?;
  save(api, &renewed)?;
  Ok(renewed)
}

/// Keep the token with the session, and write the session file at once.
fn save(api: &Api, token: &str) -> Result<()> {
  let now = Timestamp::now().as_second().to_string();
  api.ctx.set_extra(graph::TOKEN_KEY, token);
  api.ctx.set_extra(REFRESHED_KEY, &now);
  api.ctx.store.update_session(|session| {
    session
      .extra
      .insert(graph::TOKEN_KEY.to_owned(), token.to_owned());
    session.extra.insert(REFRESHED_KEY.to_owned(), now.clone());
  })
}
