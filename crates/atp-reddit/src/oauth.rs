//! OAuth "script" app credentials: a password-grant token for
//! `https://oauth.reddit.com`, cached until shortly before it expires.
//!
//! Active when `REDDIT_CLIENT_ID`, `REDDIT_CLIENT_SECRET`, `REDDIT_USERNAME`
//! and `REDDIT_PASSWORD` are all set (an app of type "script" from
//! <https://www.reddit.com/prefs/apps>). The token is never logged.

use atp_core::credentials::env;
use atp_core::token::AccessToken;
use atp_core::{Ctx, Error, Result, ValueExt};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

const TOKEN_URL: &str = "https://www.reddit.com/api/v1/access_token";
const ENV_HINT: &str = "check REDDIT_CLIENT_ID, REDDIT_CLIENT_SECRET, REDDIT_USERNAME and \
   REDDIT_PASSWORD (a \"script\" app from https://www.reddit.com/prefs/apps)";

pub struct Credentials {
  client_id: String,
  secret: String,
  pub username: String,
  password: String,
  token: AccessToken,
}

impl Credentials {
  pub fn from_env() -> Option<Self> {
    // The password is taken as typed: spaces may belong to it.
    let password = std::env::var("REDDIT_PASSWORD")
      .ok()
      .filter(|v| !v.trim().is_empty())?;
    Some(Self {
      client_id: env("REDDIT_CLIENT_ID")?,
      secret: env("REDDIT_CLIENT_SECRET")?,
      username: env("REDDIT_USERNAME")?,
      password,
      token: AccessToken::default(),
    })
  }

  /// Reddit asks API clients for a unique, descriptive User-Agent.
  pub fn user_agent(&self) -> String {
    format!(
      "cli:all-the-platforms:{} (by /u/{})",
      env!("CARGO_PKG_VERSION"),
      self.name()
    )
  }

  /// The username reduced to characters safe in headers and file names.
  fn name(&self) -> String {
    self
      .username
      .chars()
      .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
      .collect()
  }

  fn cache_key(&self) -> String {
    format!("oauth-{}", self.name().to_ascii_lowercase())
  }

  /// A valid bearer token: from memory, the cache, or a new password grant.
  pub async fn token(&self, ctx: &Ctx) -> Result<String> {
    let key = self.cache_key();
    self
      .token
      .get(ctx, &key, async || self.grant(ctx).await)
      .await
  }

  /// Forget the token (it was refused), so the next run asks for a new one.
  pub fn forget(&self, ctx: &Ctx) {
    self.token.forget(ctx, &self.cache_key());
  }

  /// A new token and its lifetime in seconds.
  async fn grant(&self, ctx: &Ctx) -> Result<(String, i64)> {
    let basic = STANDARD.encode(format!("{}:{}", self.client_id, self.secret));
    let resp = ctx
      .http
      .post(TOKEN_URL)
      .no_cookies()
      .header("authorization", format!("Basic {basic}"))
      .header("user-agent", self.user_agent())
      .form([
        ("grant_type", "password"),
        ("username", self.username.as_str()),
        ("password", self.password.as_str()),
      ])
      .send()
      .await?;
    // Bad app credentials answer 401; a bad password answers 200 with `error`.
    let v = resp.value().unwrap_or_default();
    let token = v.str("access_token").filter(|_| resp.status.is_success());
    let Some(token) = token else {
      let reason = v
        .str("error")
        .unwrap_or_else(|| format!("HTTP {}", resp.status));
      return Err(
        Error::auth(format!("Reddit refused the OAuth credentials ({reason})")).with_hint(ENV_HINT),
      );
    };
    Ok((token, v.i64("expires_in").unwrap_or(3600)))
  }
}
