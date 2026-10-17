//! Writes as the web client sends them: the ticket-guard headers its
//! secure SDK adds on the paths it guards, device traits on likes,
//! follows and comments, the secsdk CSRF token on every POST, and a pause
//! first.
//!
//! [`GUARDED`] is the web client's own list (`SecuritySDKWebConsumerPath`
//! of its security bootstrap, scene `web_protect`), narrowed to the
//! writes ATP sends; its device-trait list
//! (`SecurityDTraitSDKWebConsumerPath`) holds likes, follows and comments
//! only, so the writes of [`Api::write_as`] go without traits.

use std::time::Duration;

use atp_core::Result;
use atp_core::http::Method;

use super::{Api, Call, WWW};
use crate::sign;
use crate::sign::guard::Guard;

/// Writes the secure SDK guards: they name the ticket-guard key, so they
/// need a login made by ATP (its key). The others (removing followers,
/// blocks, unsaving several posts or mixes, watch later, clearing the
/// history) go without.
const GUARDED: &[&str] = &[
  "/aweme/v1/web/commit/item/digg/",
  "/aweme/v1/web/cancel/item/digg/multi/",
  "/aweme/v1/web/commit/follow/user/",
  "/aweme/v1/web/commit/follow/request/approve/",
  "/aweme/v1/web/comment/publish",
  "/aweme/v1/web/aweme/collect/",
  "/aweme/v1/web/collects/maintain/",
  "/aweme/v1/web/collects/video/move/",
  "/aweme/v1/web/mix/collect/",
  "/aweme/v1/web/series/collect/",
  "/aweme/v1/web/music/collect/",
];

/// How long a write waits first: Douyin flags bursts.
const PAUSE: (Duration, Duration) = (Duration::from_secs(2), Duration::from_secs(6));

impl Api {
  /// A write as a form POST to `path`, after a pause of 2 to 6 s, with
  /// the ticket-guard headers and device traits. `signed` writes
  /// (comments) carry a ticket-guard signature over the path, the others
  /// only name the key. `uid` is the MD5 of the account's uid, which the
  /// web client appends unsigned. Likes, follows, saves and comments go
  /// through here; other writes through [`write_as`](Self::write_as).
  pub async fn write(&self, path: &str, signed: bool, uid: &str) -> Result<Call<'_>> {
    self.require_login()?;
    let guard = Guard::load(&self.ctx)?;
    let mut call = self.post(path).tail("uid", uid);
    for (name, value) in guard.headers(path, signed)? {
      call = call.header(name, value);
    }
    call = call.header("x-tt-session-dtrait", sign::dtrait::header(path)?);
    self.ready(call).await
  }

  /// A write sent with `method` (a few are GETs) the way the web client
  /// sends it on `path`: the ticket-guard key where its secure SDK guards
  /// the path, no device traits, and the same pause and CSRF token as
  /// [`write`](Self::write).
  pub async fn write_as(&self, method: Method, path: &str, uid: &str) -> Result<Call<'_>> {
    self.require_login()?;
    let mut call = Call::new(self, path, method).tail("uid", uid);
    if GUARDED.contains(&path) {
      for (name, value) in Guard::load(&self.ctx)?.headers(path, false)? {
        call = call.header(name, value);
      }
    }
    self.ready(call).await
  }

  /// Wait out the pause, then add the CSRF token the secsdk puts on
  /// every POST.
  async fn ready<'a>(&'a self, mut call: Call<'a>) -> Result<Call<'a>> {
    self.ctx.http.pause(PAUSE.0, PAUSE.1).await;
    if call.method == Method::POST
      && let Some(token) = self.csrf_token().await
    {
      call = call.header("x-secsdk-csrf-token", token);
    }
    Ok(call)
  }

  /// The secsdk CSRF token, asked for with a `HEAD` request
  /// (`x-ware-csrf-token: 0,<token>,…`).
  async fn csrf_token(&self) -> Option<String> {
    let resp = self
      .ctx
      .http
      .request(Method::HEAD, format!("{WWW}/service/2/abtest_config/"))
      .header("x-secsdk-csrf-request", "1")
      .header("x-secsdk-csrf-version", "1.2.22")
      .send()
      .await
      .ok()?;
    let header = resp.header("x-ware-csrf-token")?;
    header
      .split(',')
      .nth(1)
      .filter(|t| !t.is_empty())
      .map(str::to_owned)
  }
}
