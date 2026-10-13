//! The cookie session: the account behind it, its modhash, and the web
//! app's bearer token (`token_v2`) used for writes and `oauth.reddit.com`.

use social_core::{Error, Result, Value, ValueExt};

use super::{Api, SESSION_COOKIE, USER_KEY, WWW};

impl Api {
  /// The web app's bearer token (`token_v2`) of a cookie session while it is
  /// valid; an expired one is renewed by loading the home page once.
  pub(super) async fn web_token(&self) -> Option<String> {
    if self.oauth.is_some() || !self.ctx.http.has_cookie(SESSION_COOKIE) {
      return None;
    }
    let valid = || self.ctx.http.cookie("token_v2").filter(|t| !jwt_expired(t));
    if let Some(token) = valid() {
      return Some(token);
    }
    tracing::debug!(
      "token_v2 {}; loading the home page to renew it",
      if self.ctx.http.has_cookie("token_v2") {
        "expired"
      } else {
        "missing"
      }
    );
    let renew = self
      .ctx
      .http
      .get(format!("{WWW}/"))
      .header("accept", "text/html,application/xhtml+xml")
      .send()
      .await;
    if let Err(e) = renew {
      tracing::debug!("token_v2 renewal failed: {e}");
    }
    let token = valid();
    tracing::debug!(
      "token_v2 after renewal: {}",
      if token.is_some() {
        "valid"
      } else {
        "unavailable"
      }
    );
    token
  }

  /// The logged-in account: `/api/me` (cookie session) or `/api/v1/me` (OAuth).
  pub async fn me(&self) -> Result<Value> {
    self.require_login()?;
    let me = match self.oauth {
      Some(_) => self.get("/api/v1/me", &[]).await?,
      None => self.get("/api/me", &[]).await?.at("data").clone(),
    };
    // An expired cookie session answers `{}`.
    let Some(name) = me.str("name") else {
      return Err(
        Error::auth("Reddit did not accept this session").with_hint(self.ctx.login_hint()),
      );
    };
    if let Some(m) = me.str("modhash") {
      *self.modhash.borrow_mut() = Some(m);
    }
    if self.oauth.is_none() {
      self.ctx.set_extra(USER_KEY, &name);
    }
    Ok(me)
  }

  /// Name of the logged-in account.
  pub async fn username(&self) -> Result<String> {
    if let Some(o) = &self.oauth {
      return Ok(o.username.clone());
    }
    self.require_login()?;
    match self.ctx.extra(USER_KEY).filter(|n| !n.is_empty()) {
      Some(name) => Ok(name),
      None => Ok(self.me().await?.str("name").unwrap_or_default()),
    }
  }

  /// Forget values of the previous account before new cookies are verified.
  pub fn reset_account(&self) {
    self.ctx.set_extra(USER_KEY, "");
    *self.modhash.borrow_mut() = None;
  }

  pub(super) async fn modhash(&self) -> Result<String> {
    if self.modhash.borrow().is_none() {
      self.me().await?;
    }
    self.modhash.borrow().clone().ok_or_else(|| {
      Error::auth("Reddit sent no modhash for this session").with_hint(self.ctx.login_hint())
    })
  }
}

/// Whether a JWT's `exp` is less than a minute away (or unreadable).
fn jwt_expired(token: &str) -> bool {
  use base64::Engine;
  use base64::engine::general_purpose::URL_SAFE_NO_PAD;
  let exp = token
    .split('.')
    .nth(1)
    .and_then(|p| URL_SAFE_NO_PAD.decode(p.trim_end_matches('=')).ok())
    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    .and_then(|v| v.i64("exp"));
  exp.is_none_or(|exp| exp - 60 <= jiff::Timestamp::now().as_second())
}
