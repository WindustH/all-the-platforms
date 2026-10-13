//! `login --token`: `SERVER TOKEN` (a personal access token), or `SERVER
//! LOGIN PASSWORD` for a password login that yields a session token.

use social_core::credentials;
use social_core::http::Method;
use social_core::{Error, Result, Value, ValueExt, json};

use crate::api::{Api, Creds, server_url};

/// Check the credentials, keep them, and return the account's user id.
pub async fn login(api: &Api, raw: &str) -> Result<String> {
  let fields = credentials::fields(raw);
  let creds = match fields.as_slice() {
    [server, token] => Creds {
      server: server_url(server),
      token: token.clone(),
    },
    [server, login, password] => password_login(api, &server_url(server), login, password).await?,
    _ => {
      return Err(Error::input(
        "expected 'SERVER TOKEN' or 'SERVER LOGIN PASSWORD'",
      ));
    }
  };
  let me = api
    .send(&creds, Method::GET, "/users/me", &[], None)
    .await?;
  let id = me
    .str("id")
    .ok_or_else(|| Error::upstream("no user id for the account"))?;
  api.save(&creds, &id);
  Ok(id)
}

/// `POST /users/login`: the session token comes back in the `Token` header.
async fn password_login(api: &Api, server: &str, login: &str, password: &str) -> Result<Creds> {
  let resp = api
    .ctx
    .http
    .post(format!("{server}/api/v4/users/login"))
    .no_cookies()
    .json(&json!({"login_id": login, "password": password}))
    .send()
    .await?;
  if !resp.status.is_success() {
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    return Err(
      Error::auth(format!(
        "Mattermost refused the login: {}",
        v.str("message").unwrap_or_else(|| resp.status.to_string())
      ))
      .with_hint("accounts with MFA or SSO need a personal access token instead"),
    );
  }
  let token = resp
    .header("token")
    .ok_or_else(|| Error::upstream("the login answer carried no session token"))?
    .to_owned();
  Ok(Creds {
    server: server.to_owned(),
    token,
  })
}
