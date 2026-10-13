//! Access tokens that platforms hand out for a while (app tokens, OAuth
//! tokens): kept in memory for the run and in the private cache until
//! shortly before they expire.

use std::cell::RefCell;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::ctx::Ctx;
use crate::error::Result;

/// Renew this long before the upstream expiry.
const MARGIN_SECS: i64 = 300;
/// Upper bound for a cache file's age; the stored expiry decides first.
const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

#[derive(Serialize, Deserialize)]
struct Cached {
  token: String,
  /// Unix seconds.
  expires: i64,
}

/// One access token, cached under a key that names its credentials
/// (`tenant-token-<app id>`), so different credentials never share it.
#[derive(Default)]
pub struct AccessToken {
  memo: RefCell<Option<String>>,
}

impl AccessToken {
  /// A valid token: from memory, the cache, or `fetch`, which returns the
  /// token and how many seconds it lives.
  pub async fn get(
    &self,
    ctx: &Ctx,
    key: &str,
    fetch: impl AsyncFnOnce() -> Result<(String, i64)>,
  ) -> Result<String> {
    if let Some(t) = self.memo.borrow().clone() {
      return Ok(t);
    }
    let now = jiff::Timestamp::now().as_second();
    let cached = ctx
      .store
      .cache_get::<Cached>(key, CACHE_TTL)
      .filter(|c| c.expires - MARGIN_SECS > now && !c.token.is_empty());
    let token = match cached {
      Some(c) => c.token,
      None => {
        let (token, lifetime) = fetch().await?;
        let cached = Cached {
          token: token.clone(),
          expires: now + lifetime,
        };
        ctx.store.cache_put(key, &cached);
        token
      }
    };
    *self.memo.borrow_mut() = Some(token.clone());
    Ok(token)
  }

  /// Drop the token (it was refused), so the next request fetches a new one.
  pub fn forget(&self, ctx: &Ctx, key: &str) {
    *self.memo.borrow_mut() = None;
    let empty = Cached {
      token: String::new(),
      expires: 0,
    };
    ctx.store.cache_put(key, &empty);
  }
}
