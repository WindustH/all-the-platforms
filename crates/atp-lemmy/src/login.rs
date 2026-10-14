//! Logging in: a login token (JWT) of an instance, or a user name and
//! password that the instance turns into one (`user/login`).

use atp_core::{Error, Result, ValueExt, credentials, json};

use crate::api::{self, Api, ENV_TOTP, INSTANCE_KEY, TOKEN_KEY};

/// `INSTANCE|USER|PASSWORD`, `INSTANCE|JWT`, or a bare JWT for the
/// instance in use. The parts are separated by `|` only, so a password
/// may hold `|` itself.
pub async fn token(api: &Api, raw: &str) -> Result<()> {
  let parts: Vec<&str> = raw.trim().splitn(3, '|').map(str::trim).collect();
  let (instance, token) = match parts.as_slice() {
    [instance, user, password] => {
      let instance = api::host(instance);
      let token = password_login(api, &instance, user, password).await?;
      (instance, token)
    }
    [instance, token] => (api::host(instance), (*token).to_owned()),
    [token] => (api.instance(), (*token).to_owned()),
    _ => unreachable!("splitn yields one to three parts"),
  };
  if instance.is_empty() || token.is_empty() {
    return Err(Error::input(
      "expected `login --token 'INSTANCE|USER|PASSWORD'` or `'INSTANCE|JWT'`",
    ));
  }
  credentials::save(&api.ctx, &[(INSTANCE_KEY, &instance), (TOKEN_KEY, &token)]);
  Ok(())
}

/// The JWT for a user name (or email) and password; `LEMMY_TOTP` carries
/// the code of accounts with two-factor login.
async fn password_login(api: &Api, instance: &str, user: &str, password: &str) -> Result<String> {
  let mut body = json!({"username_or_email": user, "password": password});
  if let Some(code) = credentials::env(ENV_TOTP) {
    body["totp_2fa_token"] = json!(code);
  }
  let v = api.post_to(instance, "user/login", body).await?;
  v.str("jwt").ok_or_else(|| {
    let pending =
      v.bool("registration_created") == Some(true) || v.bool("verify_email_sent") == Some(true);
    if pending {
      Error::auth("the account is not active yet (registration or email verification pending)")
    } else {
      Error::upstream("the instance answered the login without a token")
    }
  })
}
