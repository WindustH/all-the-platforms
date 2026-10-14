//! Logins, as web.okjike.com does them.
//!
//! - QR code: `GET /sessions.create` hands out a `uuid`; the QR code holds
//!   `jike://page.jk/web?url=<https://www.okjike.com/account/scan?uuid=…>`,
//!   which the Jike app opens when it scans it; `GET
//!   /sessions.wait_for_confirmation?uuid=` answers `confirmed` with a
//!   refresh token once the login is confirmed on the phone, and the web
//!   app trades it at once for a fresh pair through
//!   `app_auth_tokens.refresh`.
//! - Tokens: the web app keeps its tokens in the browser's local storage
//!   (`JK_ACCESS_TOKEN`, `JK_REFRESH_TOKEN`), not in cookies, so
//!   `--browser` cannot import them; `login --token` takes them as copied.

use std::time::Duration;

use atp_core::oauth::{ACCESS_TOKEN, EXPIRES_AT, REFRESH_TOKEN, Tokens};
use atp_core::text::encode_uri_component;
use atp_core::{Error, ErrorCode, QrStatus, QrTicket, Result, Value, ValueExt, credentials};

use crate::api::{API, Api, WEB};

/// How long one confirmation poll may wait for an answer.
const POLL_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let v = api
    .ctx
    .http
    .get(format!("{API}/sessions.create"))
    .header("origin", WEB)
    .header("platform", "web")
    .send()
    .await?
    .check()?
    .value()?;
  let uuid = v
    .str("uuid")
    .ok_or_else(|| Error::upstream("Jike's QR login gave no uuid"))?;
  let page = format!("https://www.okjike.com/account/scan?uuid={uuid}");
  Ok(QrTicket {
    url: format!(
      "jike://page.jk/web?url={}&displayHeader=false&displayFooter=false",
      encode_uri_component(&page)
    ),
    token: uuid,
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let sent = api
    .ctx
    .http
    .get(format!("{API}/sessions.wait_for_confirmation"))
    .query("uuid", &ticket.token)
    .header("origin", WEB)
    .header("platform", "web")
    .timeout(POLL_TIMEOUT)
    .retries(0)
    .send()
    .await;
  let resp = match sent {
    Ok(resp) if resp.status.is_success() => resp,
    Ok(_) => return Ok(QrStatus::Waiting),
    Err(e) if e.code == ErrorCode::NetworkError => return Ok(QrStatus::Waiting),
    Err(e) => return Err(e),
  };
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
  if v.bool("confirmed") != Some(true) {
    return Ok(QrStatus::Waiting);
  }
  let refresh_token = v
    .str("x-jike-refresh-token")
    .ok_or_else(|| Error::upstream("Jike confirmed the login without a refresh token"))?;
  let tokens = refresh(api, &refresh_token).await?;
  keep(api, &tokens);
  Ok(QrStatus::Confirmed)
}

/// `login --token`: `REFRESH_TOKEN`, or `ACCESS_TOKEN|REFRESH_TOKEN` as the
/// web app keeps them. A refresh token alone is traded for a fresh pair.
pub async fn token(api: &Api, token: &str) -> Result<()> {
  let fields = credentials::expect(token, 1, 2, "REFRESH_TOKEN or ACCESS_TOKEN|REFRESH_TOKEN")?;
  let tokens = match fields.as_slice() {
    [refresh_token] => refresh(api, refresh_token).await?,
    [access, refresh_token] => Tokens {
      access: access.clone(),
      refresh: Some(refresh_token.clone()),
      expires_at: None,
    },
    _ => unreachable!("expect checked the count"),
  };
  keep(api, &tokens);
  Ok(())
}

/// Keep the tokens in the session the login saves.
fn keep(api: &Api, tokens: &Tokens) {
  let refresh_token = tokens.refresh.clone().unwrap_or_default();
  credentials::save(
    &api.ctx,
    &[
      (ACCESS_TOKEN, tokens.access.as_str()),
      (REFRESH_TOKEN, refresh_token.as_str()),
      (EXPIRES_AT, ""),
    ],
  );
}

/// A fresh access token and the refresh token that replaces `refresh_token`.
pub async fn refresh(api: &Api, refresh_token: &str) -> Result<Tokens> {
  let resp = api
    .ctx
    .http
    .post(format!("{API}/app_auth_tokens.refresh"))
    .json(&serde_json::json!({}))
    .header("origin", WEB)
    .header("platform", "web")
    .header("x-jike-refresh-token", refresh_token)
    .send()
    .await?;
  if matches!(resp.status.as_u16(), 401 | 403) {
    return Err(Error::auth("Jike refused the refresh token"));
  }
  let v = resp.check()?.value()?;
  let access = v
    .str("x-jike-access-token")
    .ok_or_else(|| Error::auth("Jike renewed no access token"))?;
  Ok(Tokens {
    access,
    refresh: v
      .str("x-jike-refresh-token")
      .or_else(|| Some(refresh_token.to_owned())),
    expires_at: None,
  })
}
