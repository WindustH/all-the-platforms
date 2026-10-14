//! Logging in with the user's own Spotify app: Authorization Code with
//! PKCE (no client secret). The consent page opens in the default browser,
//! which comes back to `http://127.0.0.1:PORT/callback` (the redirect URI
//! the app registers), where a one-shot listener takes the code; the code
//! and the PKCE verifier become the tokens, kept with the session together
//! with the client id that renews them.

use atp_core::oauth::{self, Via};
use atp_core::{Error, ErrorCode, Result, credentials};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::api::{Api, CLIENT_ID, ENV_CLIENT_ID};

const AUTHORIZE_URL: &str = "https://accounts.spotify.com/authorize";
/// The permissions of the commands ATP has, and no others.
const SCOPES: &[&str] = &[
  "user-read-private",
  "user-library-read",
  "user-library-modify",
  "playlist-read-private",
  "playlist-modify-public",
  "playlist-modify-private",
  "user-follow-read",
  "user-follow-modify",
  "user-top-read",
  "user-read-recently-played",
  "user-read-playback-state",
  "user-modify-playback-state",
  "user-read-currently-playing",
];
pub const DEFAULT_PORT: u16 = 8888;
pub const ENV_PORT: &str = "SPOTIFY_REDIRECT_PORT";

/// `login --token CLIENT_ID` (or `env` for `SPOTIFY_CLIENT_ID`): the
/// consent of the user's own app in the browser.
pub async fn token(api: &Api, arg: &str) -> Result<()> {
  let client = client_id(arg)?;
  let port = oauth::port(ENV_PORT, DEFAULT_PORT)?;
  let redirect = format!("http://127.0.0.1:{port}/callback");
  let listener = oauth::listen(port, ENV_PORT).await.map_err(|e| {
    e.with_hint(format!(
      "free the port, or set {ENV_PORT} and register http://127.0.0.1:PORT/callback in the app"
    ))
  })?;
  let verifier = oauth::random(64);
  let state = oauth::random(24);
  let url = consent_url(&client, &redirect, &challenge(&verifier), &state)?;
  oauth::open_consent(&url);
  atp_core::output::note(&format!(
    "The app must list {redirect} under Redirect URIs; if Spotify says \"Invalid redirect \
     URI\", add it in the dashboard, stop this with Ctrl-C and log in again."
  ));
  let code = oauth::wait(&listener, "/callback", &state, Via::Query, "Spotify")
    .await?
    .remove("code")
    .ok_or_else(|| {
      Error::auth("the browser came back from another login attempt; run login again")
    })?;
  let form = [
    ("grant_type", "authorization_code"),
    ("code", code.as_str()),
    ("redirect_uri", redirect.as_str()),
    ("client_id", client.as_str()),
    ("code_verifier", verifier.as_str()),
  ];
  let tokens = api.grant(&form).await.map_err(|e| match e.code {
    ErrorCode::NotAuthenticated => e.with_hint(format!(
      "check the Client ID of your app, and that its Redirect URIs list {redirect}"
    )),
    _ => e,
  })?;
  // On disk at once like the tokens: they renew only through this app.
  credentials::save(&api.ctx, &[(CLIENT_ID, &client)]);
  api.ctx.store.update_session(|session| {
    session.extra.insert(CLIENT_ID.to_owned(), client.clone());
  })?;
  tokens.save(&api.ctx)
}

/// The client id given, or the environment's for `env`.
fn client_id(arg: &str) -> Result<String> {
  if arg.eq_ignore_ascii_case("env") {
    return credentials::env(ENV_CLIENT_ID)
      .ok_or_else(|| Error::input(format!("set {ENV_CLIENT_ID}, or pass `--token CLIENT_ID`")));
  }
  let fields = credentials::expect(arg, 1, 1, "CLIENT_ID")?;
  let id = fields[0].clone();
  if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
    return Err(Error::input(format!(
      "not a Spotify client id: {id} (32 hexadecimal digits, from your app's page in the \
       developer dashboard)"
    )));
  }
  Ok(id)
}

fn consent_url(client: &str, redirect: &str, challenge: &str, state: &str) -> Result<String> {
  let scope = SCOPES.join(" ");
  let params = [
    ("client_id", client),
    ("response_type", "code"),
    ("redirect_uri", redirect),
    ("code_challenge_method", "S256"),
    ("code_challenge", challenge),
    ("scope", scope.as_str()),
    ("state", state),
  ];
  let url = url::Url::parse_with_params(AUTHORIZE_URL, params)
    .map_err(|e| Error::internal(e.to_string()))?;
  Ok(url.into())
}

/// The S256 challenge of a verifier: base64url of its SHA-256, unpadded.
fn challenge(verifier: &str) -> String {
  URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
