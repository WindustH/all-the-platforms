//! `login --token`: `SERVER USER_ID TOKEN` (a personal access token) or
//! `SERVER USERNAME PASSWORD` (a password login, whose auth token is kept).

use atp_core::credentials;
use atp_core::http::Method;
use atp_core::{Error, ErrorCode, Result, ValueExt, json};

use crate::api::{Api, Body, Creds, server_url};

pub async fn login(api: &Api, raw: &str) -> Result<()> {
  let fields = credentials::fields(raw);
  let [server, first, second] = fields.as_slice() else {
    return Err(Error::input(
      "expected 'SERVER USER_ID TOKEN' or 'SERVER USERNAME PASSWORD'",
    ));
  };
  let server = server_url(server);
  let as_token = Creds {
    server: server.clone(),
    user_id: first.clone(),
    token: second.clone(),
  };
  let creds = match api.send(&as_token, Method::GET, "me", &[], None).await {
    Ok(_) => as_token,
    Err(e) if e.code == ErrorCode::NotAuthenticated => {
      password_login(api, &server, first, second).await?
    }
    Err(e) => return Err(e),
  };
  api.save(&creds);
  Ok(())
}

/// `POST /login` with a username and password; its user id and auth token.
async fn password_login(api: &Api, server: &str, user: &str, password: &str) -> Result<Creds> {
  let anonymous = Creds {
    server: server.to_owned(),
    user_id: String::new(),
    token: String::new(),
  };
  let body = Body::Json(json!({"user": user, "password": password}));
  let v = api
    .send(&anonymous, Method::POST, "login", &[], Some(body))
    .await
    .map_err(|e| {
      e.with_hint("neither a user id + token nor a username + password; accounts with 2FA need a personal access token")
    })?;
  Ok(Creds {
    server: server.to_owned(),
    user_id: v
      .str("data.userId")
      .ok_or_else(|| Error::upstream("the login answer carried no user id"))?,
    token: v
      .str("data.authToken")
      .ok_or_else(|| Error::upstream("the login answer carried no token"))?,
  })
}
