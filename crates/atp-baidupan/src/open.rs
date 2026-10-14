//! The open platform: Baidu's documented API for apps
//! (`pan.baidu.com/rest/2.0/xpan/*`, `/api/quota`, the upload hosts), with
//! the OAuth token of the user's own app in the query, `User-Agent:
//! pan.baidu.com`, and never the web cookies.
//!
//! The app's key and secret come from `login --token` (kept with the
//! session) or from `BAIDUPAN_APP_KEY` / `BAIDUPAN_SECRET_KEY`; atp ships
//! no key of its own. The access token lasts 30 days; the refresh token
//! that renews it works once, so every new pair is written to the session
//! file the moment it arrives ([`oauth::access_token`]).

use atp_core::http::{Method, Req, status_error};
use atp_core::oauth::{self, Tokens};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, credentials, task};

use crate::api::{OPEN_HINT, Route, check, retryable};

pub const XPAN: &str = "https://pan.baidu.com/rest/2.0/xpan";
const OAUTH: &str = "https://openapi.baidu.com/oauth/2.0";
/// The User-Agent the open platform asks for on calls and downloads.
pub const UA: &str = "pan.baidu.com";
/// Session extras: the app the tokens were issued to.
pub const APP_KEY: &str = "app_key";
pub const SECRET_KEY: &str = "secret_key";
pub const ENV_KEY: &str = "BAIDUPAN_APP_KEY";
pub const ENV_SECRET: &str = "BAIDUPAN_SECRET_KEY";
/// Calls the per-minute limit turns away get this many tries in all.
const TRIES: u32 = 2;

pub struct Open<'a> {
  ctx: &'a Ctx,
}

impl<'a> Open<'a> {
  pub fn new(ctx: &'a Ctx) -> Self {
    Self { ctx }
  }

  /// Whether a token is saved (not whether it still works).
  pub fn logged_in(&self) -> bool {
    Tokens::saved(self.ctx).is_some()
  }

  /// Fail unless the open-platform login is there; `what` names the command.
  pub fn require(&self, what: &str) -> Result<()> {
    if self.logged_in() {
      return Ok(());
    }
    Err(
      Error::auth(format!(
        "{what} goes only through Baidu's open platform, which needs your own app's login"
      ))
      .with_hint(OPEN_HINT),
    )
  }

  /// The app's key and secret: those the tokens were issued to, else the
  /// environment's.
  pub fn app(&self) -> Option<(String, String)> {
    if let Some(saved) = credentials::saved(self.ctx, &[APP_KEY, SECRET_KEY]) {
      return Some((saved[0].clone(), saved[1].clone()));
    }
    Some((credentials::env(ENV_KEY)?, credentials::env(ENV_SECRET)?))
  }

  /// A usable access token, renewed (and saved) when it is about to expire.
  pub async fn token(&self) -> Result<String> {
    oauth::access_token(self.ctx, async |refresh| self.refresh(refresh).await)
      .await
      .map_err(|e| match e.code {
        ErrorCode::NotAuthenticated => e.with_hint(OPEN_HINT),
        _ => e,
      })
  }

  async fn refresh(&self, refresh_token: &str) -> Result<Tokens> {
    let Some((key, secret)) = self.app() else {
      return Err(Error::auth(format!(
        "the app key and secret that renew the token are missing (set {ENV_KEY} and {ENV_SECRET})"
      )));
    };
    let query = [
      ("grant_type", "refresh_token".to_owned()),
      ("refresh_token", refresh_token.to_owned()),
      ("client_id", key),
      ("client_secret", secret),
    ];
    Tokens::parse(&self.oauth("token", &query).await?)
  }

  /// `openapi.baidu.com/oauth/2.0/<path>`: its JSON, whatever the status
  /// (OAuth errors come as 400 with a body).
  pub async fn oauth(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let resp = self
      .ctx
      .http
      .get(format!("{OAUTH}/{path}"))
      .no_cookies()
      .header("user-agent", UA)
      .queries(query.iter().cloned())
      .send()
      .await?;
    resp
      .value()
      .map_err(|_| status_error(resp.status, &resp.text()))
  }

  /// A GET (`path` under `rest/2.0/xpan`, or a whole URL).
  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.send(Method::GET, path, query, None).await
  }

  /// A form POST (`path` under `rest/2.0/xpan`, or a whole URL).
  pub async fn post(
    &self,
    path: &str,
    query: &[(&str, String)],
    form: &[(&str, String)],
  ) -> Result<Value> {
    self.send(Method::POST, path, query, Some(form)).await
  }

  async fn send(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    form: Option<&[(&str, String)]>,
  ) -> Result<Value> {
    let token = self.token().await?;
    let what = query
      .iter()
      .find(|(k, _)| *k == "method")
      .map_or_else(|| short(path), |(_, m)| m.clone());
    task::retry(TRIES, retryable, async || {
      let mut req = self
        .request(method.clone(), path, &token)
        .queries(query.iter().cloned());
      if let Some(form) = form {
        req = req.form(form.iter().cloned());
      }
      check(&req.send().await?, &what, Route::Open)
    })
    .await
  }

  /// A request with the token and the open platform's User-Agent, without
  /// the session cookies.
  pub fn request(&self, method: Method, path: &str, token: &str) -> Req<'a> {
    let url = if path.starts_with("https://") {
      path.to_owned()
    } else {
      format!("{XPAN}{path}")
    };
    self
      .ctx
      .http
      .request(method, url)
      .no_cookies()
      .header("user-agent", UA)
      .query("access_token", token)
  }
}

/// A call's name in messages: its path without the scheme and host.
fn short(path: &str) -> String {
  match path.strip_prefix("https://") {
    Some(rest) => rest.find('/').map_or(rest, |i| &rest[i..]).to_owned(),
    None => path.to_owned(),
  }
}
