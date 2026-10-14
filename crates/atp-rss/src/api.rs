//! RSS transport: feeds are fetched and parsed by `atp_core::feed`
//! (conditional requests against a private cache, feed discovery on web
//! pages); RSSHub routes become feed URLs on the user's chosen instance;
//! a Miniflux account, when logged in, is the [`Miniflux`] route.

use atp_core::{Ctx, ErrorCode, Result, credentials, feed};

use crate::miniflux::Miniflux;
use crate::refs;

pub const ENV_HUB: &str = "RSSHUB_INSTANCE";
pub const DEFAULT_HUB: &str = "https://rsshub.app";

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  /// The RSSHub instance: `RSSHUB_INSTANCE`, else rsshub.app.
  pub fn hub(&self) -> String {
    credentials::env(ENV_HUB)
      .unwrap_or_else(|| DEFAULT_HUB.to_owned())
      .trim_end_matches('/')
      .to_owned()
  }

  /// The Miniflux account, when one is logged in (or given by the environment).
  pub fn miniflux(&self) -> Option<Miniflux<'_>> {
    Miniflux::from_session(&self.ctx)
  }

  pub fn require_miniflux(&self) -> Result<Miniflux<'_>> {
    self.miniflux().ok_or_else(|| {
      atp_core::Error::auth("this needs a Miniflux account")
        .with_hint("run `atp rss login --token 'URL|API_KEY'` (see `atp guide login rss`)")
    })
  }

  /// A feed argument as a URL (`rsshub:/route` on the instance in use).
  pub fn feed_url(&self, arg: &str) -> Result<String> {
    refs::feed(arg, &self.hub())
  }

  /// Load a feed given as an argument.
  pub async fn load(&self, arg: &str) -> Result<feed::Feed> {
    let url = self.feed_url(arg)?;
    let hub = self.hub();
    feed::load(&self.ctx, &url).await.map_err(|e| {
      let refused = matches!(e.code, ErrorCode::PermissionDenied | ErrorCode::RateLimited);
      if refused && url.starts_with(&hub) {
        e.with_hint(format!(
          "{hub} turned ATP away; set RSSHUB_INSTANCE to your own RSSHub (or another public \
           instance), or use `hub ROUTE --instance URL`"
        ))
      } else {
        e
      }
    })
  }
}
