//! Threads transport: the Threads API (`graph.threads.net/v1.0`) through
//! atp-meta's [`Graph`], authorized by a long-lived user token from your
//! own Meta app. The token comes from `THREADS_ACCESS_TOKEN` when set, else
//! from the session (`login --token`); a saved token is renewed
//! (`th_refresh_token`) once it is a week old, so it never reaches its
//! 60 days while ATP is in use.
//!
//! The API has no lookup by shortcode, so the links of posts ATP has
//! listed are remembered (shortcode → id, in the private cache).

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::{Ctx, Error, Post, Result, ValueExt, credentials};
use atp_meta::Graph;
use atp_meta::graph::{self, fields};
use jiff::Timestamp;

use crate::login;

pub const ENV_TOKEN: &str = "THREADS_ACCESS_TOKEN";
pub const USER_ID_KEY: &str = "user_id";
pub const USERNAME_KEY: &str = "username";
/// When the saved token was last renewed (Unix seconds).
pub const REFRESHED_KEY: &str = "token_refreshed_at";
/// Renew a saved token once it is this old.
const REFRESH_AFTER_SECS: i64 = 7 * 24 * 3600;
/// Shortcodes → ids of listed posts.
const IDS_CACHE: &str = "post-ids";
const IDS_TTL: Duration = Duration::from_secs(90 * 24 * 3600);
const IDS_MAX: usize = 5000;

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  pub fn token(&self) -> Option<String> {
    graph::token(&self.ctx, &[ENV_TOKEN]).filter(|t| !t.is_empty())
  }

  pub fn logged_in(&self) -> bool {
    self.token().is_some()
  }

  /// The Threads API with a usable token (renewed first when due).
  pub async fn graph(&self) -> Result<Graph<'_>> {
    let token = self.token().ok_or_else(|| {
      Error::auth("no Threads access token")
        .with_hint("run `atp threads login --token ACCESS_TOKEN` (see `atp guide login threads`)")
    })?;
    if credentials::env(ENV_TOKEN).is_some() || !self.refresh_due() {
      return Ok(Graph::threads(&self.ctx, token));
    }
    match login::refresh(self, &token).await {
      Ok(renewed) => Ok(Graph::threads(&self.ctx, renewed)),
      Err(e) => {
        tracing::debug!("renewing the Threads token failed: {}", e.message);
        Ok(Graph::threads(&self.ctx, token))
      }
    }
  }

  fn refresh_due(&self) -> bool {
    let at: i64 = self
      .ctx
      .extra(REFRESHED_KEY)
      .and_then(|t| t.parse().ok())
      .unwrap_or(0);
    Timestamp::now().as_second() - at > REFRESH_AFTER_SECS
  }

  /// Your Threads user id, from the session or `/me`.
  pub async fn me(&self) -> Result<String> {
    match self.ctx.extra(USER_ID_KEY).filter(|id| !id.is_empty()) {
      Some(id) => Ok(id),
      None => Ok(self.lookup_me().await?.0),
    }
  }

  /// Your username, from the session or `/me`.
  pub async fn username(&self) -> Result<String> {
    match self.ctx.extra(USERNAME_KEY).filter(|name| !name.is_empty()) {
      Some(name) => Ok(name),
      None => Ok(self.lookup_me().await?.1),
    }
  }

  /// `/me`: your id and username, kept with the session.
  async fn lookup_me(&self) -> Result<(String, String)> {
    let v = self
      .graph()
      .await?
      .get("me", &fields("id,username"))
      .await?;
    let id = v
      .str("id")
      .ok_or_else(|| Error::upstream("the token names no Threads account"))?;
    let name = v.str("username").unwrap_or_default();
    self.ctx.set_extra(USER_ID_KEY, &id);
    self.ctx.set_extra(USERNAME_KEY, &name);
    Ok((id, name))
  }

  /// A short pause before a change.
  pub async fn pause(&self) {
    self
      .ctx
      .http
      .pause(Duration::from_millis(500), Duration::from_millis(1500))
      .await;
  }

  /// Remember the shortcodes of listed posts, so their links work later.
  pub fn remember(&self, posts: &[Post]) {
    let mut ids = self.ids();
    let before = ids.len();
    for post in posts {
      if let Some(code) = post.extra.get("shortcode").and_then(|c| c.as_str()) {
        ids.insert(code.to_owned(), post.id.clone());
      }
    }
    if ids.len() == before {
      return;
    }
    if ids.len() > IDS_MAX {
      ids.clear();
    }
    self.ctx.store.cache_put(IDS_CACHE, &ids);
  }

  /// The id of a shortcode ATP has listed before.
  pub fn known(&self, code: &str) -> Option<String> {
    self.ids().remove(code)
  }

  fn ids(&self) -> BTreeMap<String, String> {
    self
      .ctx
      .store
      .cache_get(IDS_CACHE, IDS_TTL)
      .unwrap_or_default()
  }
}
