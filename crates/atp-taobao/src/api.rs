//! Taobao transport: the mtop gateway (`h5api.m.taobao.com`, JSON over GET,
//! signed with the `_m_h5_tk` token from atp-alibaba), item pages, and the
//! IMPaaS chat socket with its access token. Taobao serves nothing to
//! guests (the gateway answers them with a deny page), so every call needs
//! a login. [`Api`] owns the context.

use atp_alibaba::account;
use atp_alibaba::impaas::{self, CNTAOBAO, Im};
use atp_alibaba::mtop::{self, Call};
use atp_core::token::AccessToken;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

pub const WWW: &str = "https://www.taobao.com";
/// Cookies of a logged-in session.
pub const LOGIN_COOKIES: &[&str] = account::LOGIN_COOKIES;
/// The app key the chat access token is issued for.
const IM_APP_KEY: &str = "3ce2dacdc7c0c43ad7bc7f9bc7d7a1b8";
/// How long a chat access token is used before a new one is fetched.
const IM_TOKEN_LIFE: i64 = 6 * 3600;
const DOCUMENT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";

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

  /// A signed call made from the page `referer`; its `data`.
  pub async fn call(&self, api: &str, version: &str, data: Value, referer: &str) -> Result<Value> {
    self
      .mtop(Call::new(api, version, &data).header("referer", referer))
      .await
  }

  /// A signed call built by the caller; its `data`.
  pub async fn mtop(&self, call: Call<'_>) -> Result<Value> {
    self.ctx.require_login(LOGIN_COOKIES)?;
    mtop::call(&self.ctx, &mtop::TAOBAO, &call)
      .await
      .map_err(|e| self.hinted(e))
  }

  fn hinted(&self, e: Error) -> Error {
    if e.code == ErrorCode::NotAuthenticated && e.hint.is_none() {
      return e.with_hint(self.ctx.login_hint());
    }
    e
  }

  /// An item's page as a browser opens it, with the session cookies.
  /// Guests meet a login page; suspicious clients a slider.
  pub async fn item_page(&self, id: &str) -> Result<String> {
    self.ctx.require_login(LOGIN_COOKIES)?;
    let resp = self
      .ctx
      .http
      .get(item_url(id))
      .header("accept", DOCUMENT)
      .header("referer", format!("{WWW}/"))
      .send()
      .await?;
    let html = resp.text();
    if resp.url.contains("login.taobao.com") || html.contains("login.taobao.com/member/login") {
      return Err(
        Error::auth("Taobao shows item pages to logged-in users only")
          .with_hint(self.ctx.login_hint()),
      );
    }
    let walled = ["_____tmd_____", "/punish"]
      .iter()
      .any(|w| resp.url.contains(w))
      || ["_____tmd_____/punish", "x5secdata"]
        .iter()
        .any(|w| html.contains(w));
    if walled {
      return Err(
        Error::new(
          ErrorCode::VerificationRequired,
          "Taobao asks for a slider check",
        )
        .with_hint(format!(
          "open {} in a browser, pass the check, then run `atp taobao login --browser`",
          item_url(id)
        )),
      );
    }
    Ok(html)
  }

  /// Whether the session works: a chat access token is only issued to it.
  pub async fn verify(&self) -> Result<()> {
    let me = self.me()?;
    let device = impaas::device(&self.ctx, &CNTAOBAO, &me);
    self.issue(&device).await.map(drop)
  }

  /// A registered chat connection.
  pub async fn im(&self) -> Result<Im> {
    let me = self.me()?;
    let device = impaas::device(&self.ctx, &CNTAOBAO, &me);
    let key = format!("taobao-im-{me}");
    let token = self
      .im_token
      .get(&self.ctx, &key, async || self.issue(&device).await)
      .await?;
    match Im::open(&self.ctx, &CNTAOBAO, &token, &device).await {
      Err(e) if e.code == ErrorCode::NotAuthenticated => {
        self.im_token.forget(&self.ctx, &key);
        let token = self
          .im_token
          .get(&self.ctx, &key, async || self.issue(&device).await)
          .await?;
        Im::open(&self.ctx, &CNTAOBAO, &token, &device).await
      }
      other => other,
    }
  }

  /// A new chat access token and how long to use it.
  async fn issue(&self, device: &str) -> Result<(String, i64)> {
    let data =
      json!({"domain": "cntaobao", "deviceId": device, "locale": "zh_CN", "imAppKey": IM_APP_KEY});
    let v = self
      .call(
        "mtop.taobao.login.token.get.h5",
        "2.0",
        data,
        "https://market.m.taobao.com/",
      )
      .await?;
    let token = v
      .first_str(&["result.accessToken", "accessToken"])
      .ok_or_else(|| Error::upstream("Taobao issued no chat access token"))?;
    Ok((token, IM_TOKEN_LIFE))
  }
}

/// `https://item.taobao.com/item.htm?id=<id>` (Tmall items redirect on).
pub fn item_url(id: &str) -> String {
  format!("https://item.taobao.com/item.htm?id={id}")
}
