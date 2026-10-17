//! OAuth for the official YouTube APIs (Analytics, and the Data API for
//! `edit`): a Google "Desktop app" client (`YOUTUBE_CLIENT_ID`,
//! `YOUTUBE_CLIENT_SECRET`) and a refresh token (`YOUTUBE_REFRESH_TOKEN`)
//! granted once with `atp youtube oauth`. Access tokens are cached until
//! shortly before they expire; none are logged.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use atp_core::http::Req;
use atp_core::token::AccessToken;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, credentials, json};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const TOKEN_INFO_URL: &str = "https://oauth2.googleapis.com/tokeninfo";
/// The YouTube Data API v3.
pub const DATA: &str = "https://www.googleapis.com/youtube/v3";
/// Reading the analytics and the channel, and managing the channel's
/// videos (`edit`).
const SCOPES: &str = "https://www.googleapis.com/auth/yt-analytics.readonly \
  https://www.googleapis.com/auth/youtube.readonly \
  https://www.googleapis.com/auth/youtube.force-ssl";
/// Scopes that let a token change videos.
const WRITE_SCOPES: [&str; 2] = [
  "https://www.googleapis.com/auth/youtube.force-ssl",
  "https://www.googleapis.com/auth/youtube",
];
const CACHE_KEY: &str = "oauth-token";
/// How long `oauth` waits for the browser to come back.
const CONSENT_WAIT: Duration = Duration::from_secs(300);
pub const SETUP_HINT: &str = "creator analytics and video edits use the official YouTube APIs: \
  create an OAuth client of type \"Desktop app\" in a Google Cloud project with the YouTube \
  Analytics API and YouTube Data API v3 enabled, set YOUTUBE_CLIENT_ID and \
  YOUTUBE_CLIENT_SECRET, run `atp youtube oauth` and set the YOUTUBE_REFRESH_TOKEN it prints";

/// The OAuth client from the environment.
fn client() -> Option<(String, String)> {
  Some((
    credentials::env("YOUTUBE_CLIENT_ID")?,
    credentials::env("YOUTUBE_CLIENT_SECRET")?,
  ))
}

pub struct Credentials {
  id: String,
  secret: String,
  refresh: String,
  token: AccessToken,
}

impl Credentials {
  pub fn from_env() -> Option<Self> {
    let (id, secret) = client()?;
    Some(Self {
      id,
      secret,
      refresh: credentials::env("YOUTUBE_REFRESH_TOKEN")?,
      token: AccessToken::default(),
    })
  }

  /// The cache key of this grant's access token: tokens of another
  /// refresh token (another grant, other scopes) are never reused.
  fn cache_key(&self) -> String {
    let mut hasher = DefaultHasher::new();
    self.refresh.hash(&mut hasher);
    format!("{CACHE_KEY}-{:016x}", hasher.finish())
  }

  /// A valid access token: from memory, the cache or a refresh.
  pub async fn token(&self, ctx: &Ctx) -> Result<String> {
    let refresh = async || {
      let form = [
        ("client_id", self.id.as_str()),
        ("client_secret", self.secret.as_str()),
        ("refresh_token", self.refresh.as_str()),
        ("grant_type", "refresh_token"),
      ];
      let v = grant(ctx, &form).await?;
      let token = v.str("access_token").unwrap_or_default();
      Ok((token, v.i64("expires_in").unwrap_or(3600)))
    };
    self.token.get(ctx, &self.cache_key(), refresh).await
  }

  /// Forget a refused token, so the next run refreshes it.
  pub fn forget(&self, ctx: &Ctx) {
    self.token.forget(ctx, &self.cache_key());
  }

  /// Whether the grant may change videos, as Google's token info lists its
  /// scopes; `None` when that does not answer.
  pub async fn can_write(&self, ctx: &Ctx) -> Result<Option<bool>> {
    let token = self.token(ctx).await?;
    let resp = ctx
      .http
      .post(TOKEN_INFO_URL)
      .no_cookies()
      .form([("access_token", token.as_str())])
      .send()
      .await?;
    let v = resp.value().unwrap_or_default();
    let Some(scopes) = v.str("scope").filter(|_| resp.status.is_success()) else {
      return Ok(None);
    };
    let write = scopes
      .split_whitespace()
      .any(|scope| WRITE_SCOPES.contains(&scope));
    Ok(Some(write))
  }

  /// `req` with the access token: its JSON answer, or Google's error with
  /// `api` (`YouTube Analytics`) naming the API.
  pub async fn send(&self, ctx: &Ctx, req: Req<'_>, api: &str) -> Result<Value> {
    let token = self.token(ctx).await?;
    let resp = req
      .no_cookies()
      .header("authorization", format!("Bearer {token}"))
      .send()
      .await?;
    let v = resp.value().unwrap_or_default();
    if resp.status.is_success() {
      return Ok(v);
    }
    let message = v
      .str("error.message")
      .unwrap_or_else(|| format!("HTTP {}", resp.status));
    let reason = v.str("error.errors.0.reason").unwrap_or_default();
    Err(match resp.status.as_u16() {
      401 => {
        self.forget(ctx);
        Error::auth(format!("the OAuth token was refused: {message}")).with_hint(SETUP_HINT)
      }
      403 if reason == "insufficientPermissions" || message.contains("scopes") => scope_error(),
      403 => Error::new(ErrorCode::PermissionDenied, format!("{api}: {message}")),
      404 => Error::not_found(format!("{api}: {message}")),
      400 => Error::input(format!("{api}: {message}")),
      429 => Error::new(ErrorCode::RateLimited, format!("{api}: {message}")),
      _ => Error::upstream(format!("{api}: {message}")),
    })
  }
}

/// A grant made before `oauth` asked to manage videos.
pub fn scope_error() -> Error {
  Error::auth(
    "the YouTube OAuth grant may only read; editing videos needs the youtube.force-ssl scope",
  )
  .with_hint(
    "run `atp youtube oauth` again, allow access, and set the new YOUTUBE_REFRESH_TOKEN it prints",
  )
}

async fn grant(ctx: &Ctx, form: &[(&str, &str)]) -> Result<Value> {
  let resp = ctx
    .http
    .post(TOKEN_URL)
    .no_cookies()
    .form(form.iter().copied())
    .send()
    .await?;
  let v = resp.value().unwrap_or_default();
  if resp.status.is_success() && v.str("access_token").is_some() {
    return Ok(v);
  }
  let reason = v
    .first_str(&["error_description", "error"])
    .unwrap_or_else(|| format!("HTTP {}", resp.status));
  Err(Error::auth(format!("Google refused the OAuth credentials: {reason}")).with_hint(SETUP_HINT))
}

/// One-time consent: open the printed URL, allow access, and the browser
/// comes back to a local port with the code, which becomes a refresh token.
pub async fn authorize(ctx: &Ctx) -> Result<Value> {
  let (id, secret) = client().ok_or_else(|| {
    Error::input("set YOUTUBE_CLIENT_ID and YOUTUBE_CLIENT_SECRET first").with_hint(SETUP_HINT)
  })?;
  let listener = TcpListener::bind("127.0.0.1:0")?;
  listener.set_nonblocking(true)?;
  let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
  let url = url::Url::parse_with_params(
    AUTH_URL,
    [
      ("client_id", id.as_str()),
      ("redirect_uri", redirect.as_str()),
      ("response_type", "code"),
      ("scope", SCOPES),
      ("access_type", "offline"),
      ("prompt", "consent"),
    ],
  )
  .map_err(|e| Error::internal(e.to_string()))?;
  atp_core::output::note(&format!(
    "Open this URL in a browser logged in to the channel's Google account and allow access:\n{url}"
  ));
  let code = wait_for_code(&listener).await?;
  let form = [
    ("client_id", id.as_str()),
    ("client_secret", secret.as_str()),
    ("code", code.as_str()),
    ("redirect_uri", redirect.as_str()),
    ("grant_type", "authorization_code"),
  ];
  let v = grant(ctx, &form).await?;
  let refresh = v.str("refresh_token").ok_or_else(|| {
    Error::upstream("Google sent no refresh token; revoke the app's access and run oauth again")
  })?;
  Ok(json!({
    "refresh_token": refresh,
    "scope": v.str("scope"),
    "next": "export YOUTUBE_REFRESH_TOKEN=<refresh_token> (keep it secret); `insights` and `edit` then use it",
  }))
}

/// Accept the browser's redirect `GET /?code=…` and answer it.
async fn wait_for_code(listener: &TcpListener) -> Result<String> {
  use std::io::{Read, Write};
  let started = Instant::now();
  loop {
    match listener.accept() {
      Ok((mut stream, _)) => {
        stream.set_nonblocking(false)?;
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf)?;
        let request = String::from_utf8_lossy(&buf[..n]);
        let target = request.split_whitespace().nth(1).unwrap_or("/");
        let url = url::Url::parse(&format!("http://localhost{target}"))
          .map_err(|e| Error::internal(e.to_string()))?;
        let get = |k: &str| {
          url
            .query_pairs()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.into_owned())
        };
        let body = "all-the-platforms: you can close this tab.";
        let _ = write!(
          stream,
          "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\n\r\n{body}",
          body.len()
        );
        if let Some(code) = get("code") {
          return Ok(code);
        }
        if let Some(err) = get("error") {
          return Err(Error::auth(format!("consent refused: {err}")));
        }
      }
      Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
        if started.elapsed() > CONSENT_WAIT {
          return Err(Error::auth("timed out waiting for the browser"));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
      }
      Err(e) => return Err(e.into()),
    }
  }
}
