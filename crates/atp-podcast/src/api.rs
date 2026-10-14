//! Podcast transport: Apple's public iTunes Search and Lookup APIs
//! (`itunes.apple.com/search`, `/lookup`), Apple's chart feeds
//! (`rss.marketingtools.apple.com`), and Podcast Index
//! (`api.podcastindex.org`, with the user's key and secret: each request
//! signs `key + secret + unix time` with SHA-1). Show feeds themselves go
//! through `atp_core::feed`.

use atp_core::http::{TOOL_USER_AGENT, status_error};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, credentials};
use sha1::{Digest, Sha1};

pub const ITUNES: &str = "https://itunes.apple.com";
pub const CHARTS: &str = "https://rss.marketingtools.apple.com/api/v2";
pub const INDEX: &str = "https://api.podcastindex.org/api/1.0";
/// Apple Podcasts pages, for links.
pub const APPLE: &str = "https://podcasts.apple.com";
pub const KEY: &str = "index_key";
pub const SECRET: &str = "index_secret";
pub const ENV_KEY: &str = "PODCASTINDEX_KEY";
pub const ENV_SECRET: &str = "PODCASTINDEX_SECRET";
pub const ENV_COUNTRY: &str = "PODCAST_COUNTRY";

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  /// The Apple storefront searches and charts use: `PODCAST_COUNTRY`, else `us`.
  pub fn country(&self) -> String {
    credentials::env(ENV_COUNTRY)
      .map(|c| c.to_ascii_lowercase())
      .filter(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_alphabetic()))
      .unwrap_or_else(|| "us".to_owned())
  }

  /// Podcast Index key and secret: the environment, else the session.
  pub fn index_keys(&self) -> Option<(String, String)> {
    let env = credentials::env(ENV_KEY).zip(credentials::env(ENV_SECRET));
    env.or_else(|| {
      let saved = credentials::saved(&self.ctx, &[KEY, SECRET])?;
      Some((saved[0].clone(), saved[1].clone()))
    })
  }

  fn require_index(&self) -> Result<(String, String)> {
    self.index_keys().ok_or_else(|| {
      Error::auth("this needs a Podcast Index API key").with_hint(
        "get one at https://api.podcastindex.org, then `atp podcast login --token KEY:SECRET`",
      )
    })
  }

  /// `GET <ITUNES>/<path>` (search, lookup) in the storefront in use.
  pub async fn itunes(&self, path: &str, mut params: Vec<(&str, String)>) -> Result<Value> {
    params.push(("country", self.country()));
    let resp = self
      .ctx
      .http
      .get(format!("{ITUNES}/{path}"))
      .no_cookies()
      .queries(params)
      .send()
      .await?;
    if !resp.status.is_success() {
      return Err(status_error(resp.status, &resp.text()));
    }
    // Apple answers `text/javascript` with a JSON body.
    resp.value()
  }

  /// Apple's chart of `kind` (`podcasts`, `podcast-episodes`): the top `n`.
  pub async fn chart(&self, kind: &str, n: usize) -> Result<Value> {
    let url = format!("{CHARTS}/{}/podcasts/top/{n}/{kind}.json", self.country());
    self.ctx.http.get(url).no_cookies().value().await
  }

  /// `GET <INDEX>/<path>`, signed with the user's key and secret.
  pub async fn index(&self, path: &str, params: Vec<(&str, String)>) -> Result<Value> {
    let (key, secret) = self.require_index()?;
    let now = jiff::Timestamp::now().as_second().to_string();
    let digest = Sha1::digest(format!("{key}{secret}{now}").as_bytes());
    let auth: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let resp = self
      .ctx
      .http
      .get(format!("{INDEX}/{path}"))
      .no_cookies()
      .header("user-agent", TOOL_USER_AGENT)
      .header("x-auth-key", &key)
      .header("x-auth-date", &now)
      .header("authorization", auth)
      .queries(params)
      .send()
      .await?;
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    let message = v.str("description").unwrap_or_default();
    match resp.status.as_u16() {
      200..=299 if v.str("status").as_deref() != Some("false") => Ok(v),
      401 | 403 => Err(
        Error::auth(format!("Podcast Index refused the key: {message}"))
          .with_hint("check the key and secret: `atp guide login podcast`"),
      ),
      429 => Err(Error::new(
        ErrorCode::RateLimited,
        "Podcast Index: too many requests",
      )),
      200..=299 => Err(Error::upstream(format!("Podcast Index: {message}"))),
      _ => Err(status_error(resp.status, &resp.text())),
    }
  }
}
