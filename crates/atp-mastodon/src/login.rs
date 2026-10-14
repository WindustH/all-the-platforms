//! Logging in: an access token made under Preferences → Development on
//! the instance (`INSTANCE|TOKEN`), or the instance's own OAuth. For the
//! latter ATP registers itself as an app on the instance with a loopback
//! redirect, the consent page opens in the browser, and the authorization
//! code that comes back to `http://127.0.0.1:PORT/callback` becomes the
//! token. Mastodon tokens do not expire.

use atp_core::http::TOOL_USER_AGENT;
use atp_core::oauth::{self, Tokens, Via};
use atp_core::{Error, Result, Value, ValueExt, credentials};

use crate::api::Api;

pub const DEFAULT_PORT: u16 = 8897;
pub const ENV_PORT: &str = "MASTODON_REDIRECT_PORT";
/// Reading and writing everything ATP has commands for.
const SCOPES: &str = "read write";
const APP_NAME: &str = "ATP (all-the-platforms)";
const WEBSITE: &str = "https://github.com/WindustH/atp";

/// `INSTANCE|TOKEN`, a bare token for the instance in use, or `INSTANCE`
/// alone for the OAuth login.
pub async fn token(api: &Api, raw: &str) -> Result<()> {
  let parts: Vec<&str> = raw.trim().splitn(2, '|').map(str::trim).collect();
  let (instance, token) = match parts.as_slice() {
    [instance, token] => (credentials::host(instance), (*token).to_owned()),
    [one] if one.contains('.') || one.contains("://") => {
      let instance = credentials::host(one);
      let token = consent(api, &instance).await?;
      (instance, token)
    }
    [token] => (api.instance(), (*token).to_owned()),
    _ => (String::new(), String::new()),
  };
  if instance.is_empty() || token.is_empty() {
    return Err(Error::input(
      "expected `login --token INSTANCE` (OAuth) or `login --token 'INSTANCE|TOKEN'`",
    ));
  }
  verify(api, &instance, &token).await?;
  api.save(&instance, &token);
  Ok(())
}

/// Check the token against `verify_credentials` before it is saved.
async fn verify(api: &Api, instance: &str, token: &str) -> Result<()> {
  let resp = api
    .ctx
    .http
    .get(format!(
      "https://{instance}/api/v1/accounts/verify_credentials"
    ))
    .no_cookies()
    .header("user-agent", TOOL_USER_AGENT)
    .header("authorization", format!("Bearer {token}"))
    .send()
    .await?;
  if resp.status.is_success() {
    return Ok(());
  }
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
  Err(Error::auth(format!(
    "{instance} did not accept the token ({})",
    v.str("error")
      .unwrap_or_else(|| format!("HTTP {}", resp.status))
  )))
}

/// The OAuth login on `instance`: register, consent, exchange the code.
async fn consent(api: &Api, instance: &str) -> Result<String> {
  let port = oauth::port(ENV_PORT, DEFAULT_PORT)?;
  let redirect = format!("http://127.0.0.1:{port}/callback");
  let listener = oauth::listen(port, ENV_PORT).await?;
  let app = register(api, instance, &redirect).await?;
  let state = oauth::random(24);
  let params = [
    ("response_type", "code"),
    ("client_id", app.id.as_str()),
    ("redirect_uri", redirect.as_str()),
    ("scope", SCOPES),
    ("state", state.as_str()),
  ];
  let url = url::Url::parse_with_params(&format!("https://{instance}/oauth/authorize"), params)
    .map_err(|e| Error::internal(e.to_string()))?;
  oauth::open_consent(url.as_str());
  let code = oauth::wait(&listener, "/callback", &state, Via::Query, instance)
    .await?
    .remove("code")
    .ok_or_else(|| Error::auth(format!("{instance} came back without a code")))?;
  exchange(api, instance, &app, &code, &redirect).await
}

/// An app registered on the instance.
struct App {
  id: String,
  secret: String,
}

/// `POST /api/v1/apps` with the loopback redirect.
async fn register(api: &Api, instance: &str, redirect: &str) -> Result<App> {
  let form = [
    ("client_name", APP_NAME),
    ("redirect_uris", redirect),
    ("scopes", SCOPES),
    ("website", WEBSITE),
  ];
  let v = api
    .ctx
    .http
    .post(format!("https://{instance}/api/v1/apps"))
    .no_cookies()
    .header("user-agent", TOOL_USER_AGENT)
    .form(form)
    .value()
    .await
    .map_err(|e| {
      Error::upstream(format!(
        "{instance} did not register ATP as an app: {}",
        e.message
      ))
    })?;
  match (v.str("client_id"), v.str("client_secret")) {
    (Some(id), Some(secret)) => Ok(App { id, secret }),
    _ => Err(Error::upstream(format!(
      "{instance} registered ATP without a client id"
    ))),
  }
}

/// The authorization code → an access token.
async fn exchange(
  api: &Api,
  instance: &str,
  app: &App,
  code: &str,
  redirect: &str,
) -> Result<String> {
  let form = [
    ("grant_type", "authorization_code"),
    ("code", code),
    ("client_id", app.id.as_str()),
    ("client_secret", app.secret.as_str()),
    ("redirect_uri", redirect),
    ("scope", SCOPES),
  ];
  let resp = api
    .ctx
    .http
    .post(format!("https://{instance}/oauth/token"))
    .no_cookies()
    .header("user-agent", TOOL_USER_AGENT)
    .form(form)
    .send()
    .await?;
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
  Ok(Tokens::parse(&v)?.access)
}
