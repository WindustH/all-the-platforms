//! Logging in.
//!
//! - `FORUM_URL` alone: a User API Key, the way Discourse's own apps get
//!   one. ATP makes an RSA key pair and opens the forum's consent page
//!   (`/user-api-key/new`) with the public key; once you allow it, the
//!   forum hands back the key encrypted to that public key. Forums send
//!   it to a redirect address only when their admins allowed the address
//!   (`allowed_user_api_auth_redirects`; with `DISCOURSE_REDIRECT_PORT`
//!   set, ATP waits on `http://127.0.0.1:PORT/callback`), so by default
//!   the page shows the encrypted text and you paste it into the terminal.
//! - `FORUM_URL|API_KEY|USERNAME`: a key an admin made under Admin → API.

use std::io::IsTerminal;

use atp_core::oauth::{self, Via};
use atp_core::output::note;
use atp_core::{Error, Result, Value, ValueExt, credentials};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rsa::pkcs8::EncodePublicKey;
use rsa::rand_core::OsRng;
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey};

use crate::api::{self, Api, CLIENT_KEY, FORUM_KEY, KEY_KEY, USERNAME_KEY};
use crate::users;

pub const ENV_PORT: &str = "DISCOURSE_REDIRECT_PORT";
const APP_NAME: &str = "ATP (all-the-platforms)";
/// Reading, writing, notifications and who you are.
const SCOPES: &str = "read,write,notifications,session_info";
const KEY_BITS: usize = 2048;

/// `FORUM_URL` (User API Key) or `FORUM_URL|API_KEY|USERNAME`.
pub async fn token(api: &Api, raw: &str) -> Result<()> {
  let fields: Vec<&str> = raw.trim().split('|').map(str::trim).collect();
  match fields.as_slice() {
    [forum] if !forum.is_empty() => user_api_key(api, &api::forum_url(forum)).await,
    [forum, key, username] if !key.is_empty() && !username.is_empty() => {
      let forum = api::forum_url(forum);
      credentials::save(
        &api.ctx,
        &[
          (FORUM_KEY, &forum),
          (KEY_KEY, key),
          (USERNAME_KEY, username),
          (CLIENT_KEY, ""),
        ],
      );
      users::current(api).await.map(|_| ())
    }
    _ => Err(Error::input(
      "expected `login --token FORUM_URL` or `login --token 'FORUM_URL|API_KEY|USERNAME'`",
    )),
  }
}

/// The User API Key flow on `forum`.
async fn user_api_key(api: &Api, forum: &str) -> Result<()> {
  note("Making a key pair for the forum's consent page…");
  let key = RsaPrivateKey::new(&mut OsRng, KEY_BITS)
    .map_err(|e| Error::internal(format!("RSA key: {e}")))?;
  let client = oauth::random(32);
  let nonce = oauth::random(16);
  let mut params = vec![
    ("application_name", APP_NAME.to_owned()),
    ("client_id", client.clone()),
    ("scopes", SCOPES.to_owned()),
    ("public_key", public_pem(&key)?),
    ("nonce", nonce.clone()),
  ];
  let payload = match credentials::env(ENV_PORT) {
    Some(_) => {
      let port = oauth::port(ENV_PORT, 0)?;
      let listener = oauth::listen(port, ENV_PORT).await?;
      let state = oauth::random(24);
      let back = format!("http://127.0.0.1:{port}/callback?state={state}");
      params.push(("auth_redirect", back));
      oauth::open_consent(&consent_url(forum, &params)?);
      oauth::wait(&listener, "/callback", &state, Via::Query, forum)
        .await?
        .remove("payload")
        .ok_or_else(|| Error::auth("the forum came back without a key"))?
    }
    None => {
      oauth::open_consent(&consent_url(forum, &params)?);
      paste(&key)?
    }
  };
  let answer = decrypt(&key, &payload)?;
  if answer.str("nonce").as_deref() != Some(nonce.as_str()) {
    return Err(Error::auth(
      "the key came back for another login attempt; run login again",
    ));
  }
  let user_key = answer
    .str("key")
    .ok_or_else(|| Error::auth("the forum's answer held no key"))?;
  credentials::save(
    &api.ctx,
    &[
      (FORUM_KEY, forum),
      (KEY_KEY, &user_key),
      (CLIENT_KEY, &client),
    ],
  );
  let me = users::current(api).await?;
  api
    .ctx
    .set_extra(USERNAME_KEY, &me.str("username").unwrap_or_default());
  Ok(())
}

fn consent_url(forum: &str, params: &[(&str, String)]) -> Result<String> {
  let url = url::Url::parse_with_params(&format!("{forum}/user-api-key/new"), params)
    .map_err(|e| Error::input(format!("not a forum address: {forum} ({e})")))?;
  Ok(url.into())
}

/// The public key as PEM (SubjectPublicKeyInfo), as Discourse takes it.
fn public_pem(key: &RsaPrivateKey) -> Result<String> {
  let der = key
    .to_public_key()
    .to_public_key_der()
    .map_err(|e| Error::internal(format!("RSA key: {e}")))?;
  let body = STANDARD.encode(der.as_bytes());
  let lines: Vec<&str> = body
    .as_bytes()
    .chunks(64)
    .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
    .collect();
  Ok(format!(
    "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
    lines.join("\n")
  ))
}

/// Read the encrypted text the consent page shows, line by line, until it
/// is whole.
fn paste(key: &RsaPrivateKey) -> Result<String> {
  if !std::io::stdin().is_terminal() {
    return Err(Error::input(
      "the consent page shows a text to paste here; run login in a terminal",
    ));
  }
  note("Allow access on the page, then paste the text it shows here and press Enter:");
  let mut text = String::new();
  loop {
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line)? == 0 {
      break;
    }
    text.extend(line.chars().filter(|c| !c.is_whitespace()));
    if decrypt(key, &text).is_ok() || (line.trim().is_empty() && !text.is_empty()) {
      break;
    }
  }
  Ok(text)
}

/// The forum's answer: base64 of the JSON encrypted with the public key.
fn decrypt(key: &RsaPrivateKey, payload: &str) -> Result<Value> {
  let clean: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
  let data = STANDARD
    .decode(clean)
    .map_err(|_| Error::input("that is not the text the page shows (not base64)"))?;
  let json = key
    .decrypt(Pkcs1v15Encrypt, &data)
    .map_err(|_| Error::input("the text does not open with this login's key; run login again"))?;
  serde_json::from_slice(&json).map_err(|_| Error::upstream("the forum's answer is not JSON"))
}
