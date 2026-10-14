//! Spotify transport: the Web API (`api.spotify.com/v1`) with the OAuth
//! access token of the user's own app, renewed through its refresh token
//! shortly before it expires ([`oauth::access_token`]), and the token
//! endpoint of `accounts.spotify.com`. Error answers are mapped to ATP's
//! codes here, with hints for the usual causes (a development-mode app,
//! Premium, no active device).

use std::cell::RefCell;

use atp_core::http::{Method, Resp, status_error};
use atp_core::oauth::{self, Tokens};
use atp_core::{Ctx, Error, ErrorCode, PageReq, Result, Value, ValueExt, credentials};

pub const API: &str = "https://api.spotify.com/v1";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
pub const WEB: &str = "https://open.spotify.com";
/// Session extra: the client id of the app the tokens were issued to; they
/// renew only through it.
pub const CLIENT_ID: &str = "client_id";
pub const ENV_CLIENT_ID: &str = "SPOTIFY_CLIENT_ID";
/// `PUT` / `DELETE /me/library` take at most this many URIs at a time.
const LIBRARY_BATCH: usize = 40;
const DEV_MODE_HINT: &str = "add this Spotify account under User Management of your app in \
  the developer dashboard (development-mode apps allow 5 users), or log in with the app \
  owner's account";

pub struct Api {
  pub ctx: Ctx,
  /// `GET /me`, once per run.
  me: RefCell<Option<Value>>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      me: RefCell::new(None),
    }
  }

  /// Whether tokens are saved (not whether they still work).
  pub fn logged_in(&self) -> bool {
    Tokens::saved(&self.ctx).is_some()
  }

  /// The app's client id: the one the tokens belong to, else the
  /// environment's.
  pub fn client_id(&self) -> Option<String> {
    let saved = self.ctx.extra(CLIENT_ID).filter(|c| !c.is_empty());
    saved.or_else(|| credentials::env(ENV_CLIENT_ID))
  }

  /// A usable access token, renewed (and saved) when it is about to expire.
  pub async fn token(&self) -> Result<String> {
    oauth::access_token(&self.ctx, async |refresh| self.refresh(refresh).await).await
  }

  async fn refresh(&self, refresh_token: &str) -> Result<Tokens> {
    let Some(client) = self.client_id() else {
      return Err(Error::auth(format!(
        "the client id that renews the token is missing (set {ENV_CLIENT_ID})"
      )));
    };
    let form = [
      ("grant_type", "refresh_token"),
      ("refresh_token", refresh_token),
      ("client_id", client.as_str()),
    ];
    self.grant(&form).await
  }

  /// A request to the token endpoint: the tokens, or its OAuth error.
  pub async fn grant(&self, form: &[(&str, &str)]) -> Result<Tokens> {
    let resp = self
      .ctx
      .http
      .post(TOKEN_URL)
      .no_cookies()
      .form(form.iter().copied())
      .send()
      .await?;
    let v = resp
      .value()
      .map_err(|_| status_error(resp.status, &resp.text()))?;
    Tokens::parse(&v)
  }

  /// The logged-in account (`GET /me`), fetched once per run.
  pub async fn me(&self) -> Result<Value> {
    if let Some(v) = self.me.borrow().clone() {
      return Ok(v);
    }
    let v = self.get("/me", &[]).await?;
    *self.me.borrow_mut() = Some(v.clone());
    Ok(v)
  }

  /// The id of the logged-in account.
  pub async fn my_id(&self) -> Result<String> {
    let me = self.me().await?;
    me.str("id")
      .ok_or_else(|| Error::upstream("Spotify sent no account id"))
  }

  /// GET a path under `/v1` (or a `next` link Spotify sent).
  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.send(Method::GET, path, query, None).await
  }

  /// One page of an offset listing (`limit` up to `max`) and the cursor of
  /// the next page.
  pub async fn offset_page(
    &self,
    path: &str,
    query: &[(&str, String)],
    page: &PageReq,
    max: usize,
  ) -> Result<(Value, Option<String>)> {
    let offset = page.number_or(0);
    let limit = page.size_within(max);
    let mut query = query.to_vec();
    query.push(("limit", limit.to_string()));
    query.push(("offset", offset.to_string()));
    let v = self.get(path, &query).await?;
    let next = v.str("next").map(|_| (offset + limit as u64).to_string());
    Ok((v, next))
  }

  /// Save (or remove) items in the account's library by URI: liked tracks,
  /// saved albums, shows and episodes, followed artists, users and
  /// playlists.
  pub async fn library(&self, uris: &[String], remove: bool) -> Result<()> {
    let method = if remove { Method::DELETE } else { Method::PUT };
    for batch in uris.chunks(LIBRARY_BATCH) {
      let query = [("uris", batch.join(","))];
      self
        .send(method.clone(), "/me/library", &query, None)
        .await?;
    }
    Ok(())
  }

  /// Send a request with the access token; the JSON answer (`Null` for an
  /// empty one) or the error it stands for.
  pub async fn send(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
  ) -> Result<Value> {
    let url = match path.strip_prefix(API) {
      Some(rest) => format!("{API}{rest}"),
      None if path.starts_with('/') => format!("{API}{path}"),
      None => return Err(Error::internal(format!("not a Web API path: {path}"))),
    };
    let token = self.token().await?;
    let mut req = self
      .ctx
      .http
      .request(method.clone(), url)
      .no_cookies()
      .header("authorization", format!("Bearer {token}"))
      .header("accept", "application/json")
      .queries(query.iter().map(|(k, v)| (*k, v)));
    req = match body {
      Some(body) => req.json(body),
      None if method != Method::GET => req.bytes(Vec::new(), "application/json"),
      None => req,
    };
    let resp = req.send().await?;
    if !resp.status.is_success() {
      return Err(self.error(&resp));
    }
    Ok(resp.value().unwrap_or(Value::Null))
  }

  /// The error an answer stands for: `{"error": {"status", "message", "reason"}}`.
  fn error(&self, resp: &Resp) -> Error {
    let v = resp.value().unwrap_or_default();
    let message = v
      .first_str(&["error.message", "error_description", "message"])
      .unwrap_or_else(|| atp_core::text::truncate(&resp.text(), 200));
    let reason = v.str("error.reason").unwrap_or_default();
    let status = resp.status.as_u16();
    let text = format!("Spotify: {message} (HTTP {status})");
    match status {
      401 => Error::auth(text).with_hint(self.ctx.login_hint()),
      403 => self.forbidden(text, &message, &reason),
      404 if reason == "NO_ACTIVE_DEVICE" || message.contains("active device") => {
        Error::not_found(text).with_hint(NO_DEVICE_HINT)
      }
      404 => Error::not_found(text),
      429 => {
        let wait = resp.header("retry-after").unwrap_or("a few");
        Error::new(ErrorCode::RateLimited, text).with_hint(format!(
          "Spotify asks to wait {wait} seconds before the next request"
        ))
      }
      _ => Error::upstream(text),
    }
  }

  /// 403: Premium, a user the development-mode app does not know, a scope
  /// the token lacks, or something Spotify withholds from the app.
  fn forbidden(&self, text: String, message: &str, reason: &str) -> Error {
    let error = Error::new(ErrorCode::PermissionDenied, text);
    let lower = message.to_ascii_lowercase();
    if reason == "PREMIUM_REQUIRED" || lower.contains("premium") {
      error.with_hint("playback control needs Spotify Premium")
    } else if lower.contains("not registered") || lower.contains("developer dashboard") {
      error.with_hint(DEV_MODE_HINT)
    } else if lower.contains("scope") {
      error.with_hint(format!(
        "the token lacks a permission; log in again: {}",
        self.ctx.login_hint()
      ))
    } else {
      error
    }
  }
}

pub const NO_DEVICE_HINT: &str = "open Spotify on a device and play something, or pick one: \
  `atp spotify player devices`, then `--device` or `atp spotify player transfer`";
