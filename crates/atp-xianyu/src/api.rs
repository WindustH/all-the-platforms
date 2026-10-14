//! Xianyu transport: the goofish mtop gateway (signed calls, each tagged
//! with the `spm` of the web page that makes it) and the IMPaaS chat socket
//! with its access token. Logged out, the gateway hands out a signing token
//! but walls every read (`RGV587_ERROR::SM`), so calls need a login; the QR
//! login starts from the guest cookies a browser has. [`Api`] owns the
//! context.

use atp_alibaba::account;
use atp_alibaba::impaas::{self, GOOFISH, Im};
use atp_alibaba::mtop::{self, Call};
use atp_core::token::AccessToken;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

pub const WWW: &str = "https://www.goofish.com";
/// Cookies of a logged-in session.
pub const LOGIN_COOKIES: &[&str] = account::LOGIN_COOKIES;
/// The spm of the pages calls come from.
pub const HOME: &str = "a21ybx.home.0.0";
pub const SEARCH: &str = "a21ybx.search.0.0";
pub const ITEM: &str = "a21ybx.item.0.0";
pub const PERSONAL: &str = "a21ybx.personal.0.0";
pub const CHAT: &str = "a21ybx.im.0.0";
/// The app key the chat access token is issued for.
const IM_APP_KEY: &str = "444e9908a51d1cb236a27862abc769c9";
/// The access token lives a day unless the answer says otherwise.
const IM_TOKEN_LIFE: i64 = 24 * 3600;

pub struct Api {
  pub ctx: Ctx,
  im_token: AccessToken,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      im_token: AccessToken::default(),
    }
  }

  /// The logged-in account's user id.
  pub fn me(&self) -> Result<String> {
    account::me(&self.ctx)
  }

  /// A signed call (version 1.0) made from the page `spm`; its `data`.
  pub async fn call(&self, api: &str, data: Value, spm: &str) -> Result<Value> {
    self.call_version(api, "1.0", data, spm).await
  }

  pub async fn call_version(
    &self,
    api: &str,
    version: &str,
    data: Value,
    spm: &str,
  ) -> Result<Value> {
    self.ctx.require_login(LOGIN_COOKIES)?;
    let call = Call::new(api, version, &data).query("spm_cnt", spm);
    mtop::call(&self.ctx, &mtop::GOOFISH, &call)
      .await
      .map_err(|e| self.hinted(e))
  }

  /// Auth errors with the ways to log in.
  fn hinted(&self, e: Error) -> Error {
    if e.code == ErrorCode::NotAuthenticated && e.hint.is_none() {
      return e.with_hint(self.ctx.login_hint());
    }
    e
  }

  /// The cookies a browser has before logging in: `cna` (a device id from
  /// the analytics script) and a signing token. Failures are left to the
  /// requests that need them.
  pub async fn guest(&self) {
    let http = &self.ctx.http;
    if !http.has_cookie("cna") {
      let _ = http.get("https://log.mmstat.com/eg.js").send().await;
    }
    if !http.has_cookie("_m_h5_tk") {
      let _ = mtop::touch(
        &self.ctx,
        &mtop::GOOFISH,
        "mtop.taobao.idlehome.home.webpc.feed",
      )
      .await;
    }
  }

  /// A registered chat connection.
  pub async fn im(&self) -> Result<Im> {
    let me = self.me()?;
    let device = impaas::device(&self.ctx, &GOOFISH, &me);
    let key = format!("xianyu-im-{me}");
    let token = self
      .im_token
      .get(&self.ctx, &key, async || self.issue(&device).await)
      .await?;
    match Im::open(&self.ctx, &GOOFISH, &token, &device).await {
      Err(e) if e.code == ErrorCode::NotAuthenticated => {
        self.im_token.forget(&self.ctx, &key);
        let token = self
          .im_token
          .get(&self.ctx, &key, async || self.issue(&device).await)
          .await?;
        Im::open(&self.ctx, &GOOFISH, &token, &device).await
      }
      other => other,
    }
  }

  /// A new chat access token and how long it lives.
  async fn issue(&self, device: &str) -> Result<(String, i64)> {
    let data = json!({"appKey": IM_APP_KEY, "deviceId": device});
    let v = self
      .call("mtop.taobao.idlemessage.pc.login.token", data, CHAT)
      .await?;
    let token = v
      .str("accessToken")
      .ok_or_else(|| Error::upstream("Xianyu issued no chat access token"))?;
    let life = v
      .i64("accessTokenExpiredTime")
      .map(|ms| ms / 1000)
      .filter(|s| *s > 600)
      .unwrap_or(IM_TOKEN_LIFE);
    Ok((token, life))
  }

  /// Keep the session alive, as the web chat does every few minutes.
  pub async fn refresh(&self) -> Result<()> {
    self
      .call("mtop.taobao.idlemessage.pc.loginuser.get", json!({}), CHAT)
      .await
      .map(drop)
  }
}

/// `https://www.goofish.com/item?id=<id>`.
pub fn item_url(id: &str) -> String {
  format!("{WWW}/item?id={id}")
}

/// `https://www.goofish.com/personal?userId=<id>`.
pub fn user_url(id: &str) -> String {
  format!("{WWW}/personal?userId={id}")
}
