//! The iLink bot API (`ilinkai.weixin.qq.com`), the protocol of WeChat's
//! own ClawBot plugin (`@tencent-weixin/openclaw-weixin`, MIT): JSON over
//! HTTPS with a bearer `bot_token` from the QR login, a random
//! `X-WECHAT-UIN` per request, and `ret` / `errcode` in every answer
//! (`-14`: the login expired).

use std::time::Duration;

use atp_core::http::Req;
use atp_core::{Ctx, Error, Result, Value, ValueExt, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub const BASE: &str = "https://ilinkai.weixin.qq.com";
pub const CDN: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

/// Session extras of a logged-in bot.
pub const TOKEN: &str = "bot_token";
pub const BOT_ID: &str = "bot_id";
pub const OWNER: &str = "owner_id";
pub const BASE_URL: &str = "base_url";

/// How long the server may hold a long poll (it can suggest another value).
pub const LONG_POLL: Duration = Duration::from_secs(35);
const API_TIMEOUT: Duration = Duration::from_secs(20);

const SESSION_EXPIRED: i64 = -14;

/// The logged-in bot.
pub struct Account {
  pub token: String,
  pub bot_id: String,
  /// The WeChat user who scanned the code: the bot's owner.
  pub owner: String,
  pub base: String,
}

/// The transport: the context, and the bot account saved in it.
pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  /// The saved bot, if there is one.
  pub fn account(&self) -> Option<Account> {
    let ctx = &self.ctx;
    let token = ctx.extra(TOKEN).filter(|t| !t.is_empty())?;
    Some(Account {
      token,
      bot_id: ctx.extra(BOT_ID).unwrap_or_default(),
      owner: ctx.extra(OWNER).unwrap_or_default(),
      base: ctx
        .extra(BASE_URL)
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| BASE.to_owned()),
    })
  }

  /// The saved bot, or `not_authenticated`.
  pub fn require_account(&self) -> Result<Account> {
    self
      .account()
      .ok_or_else(|| Error::auth("not logged in to WeChat").with_hint(self.ctx.login_hint()))
  }

  /// POST an authenticated call and check its `ret` / `errcode`.
  pub async fn call(&self, account: &Account, endpoint: &str, body: Value) -> Result<Value> {
    let v = self
      .authorized(account, endpoint, body, API_TIMEOUT)
      .await?;
    check(endpoint, v)
  }

  /// POST without checking the answer; `None` when the client-side `wait`
  /// ran out first (normal for long polls).
  pub async fn poll(
    &self,
    account: &Account,
    endpoint: &str,
    body: Value,
    wait: Duration,
  ) -> Result<Option<Value>> {
    let call = self.authorized(account, endpoint, body, wait * 2);
    match tokio::time::timeout(wait, call).await {
      Ok(v) => v.map(Some),
      Err(_) => Ok(None),
    }
  }

  async fn authorized(
    &self,
    account: &Account,
    endpoint: &str,
    mut body: Value,
    timeout: Duration,
  ) -> Result<Value> {
    if let Some(map) = body.as_object_mut() {
      map.insert("base_info".into(), base_info());
    }
    let resp = headers(self.ctx.http.post(url(&account.base, endpoint)))
      .header("authorization", format!("Bearer {}", account.token))
      .json_text(body.to_string())
      .timeout(timeout)
      .send()
      .await?
      .check()?;
    parse_body(&resp.text())
  }

  /// GET before there is a token (the QR login); `None` when `wait` ran out.
  pub async fn get_anonymous(
    &self,
    base: &str,
    endpoint: &str,
    wait: Duration,
  ) -> Result<Option<Value>> {
    let req = headers(self.ctx.http.get(url(base, endpoint)))
      .retries(0)
      .timeout(wait * 2);
    match tokio::time::timeout(wait, req.send()).await {
      Ok(resp) => parse_body(&resp?.check()?.text()).map(Some),
      Err(_) => Ok(None),
    }
  }

  /// POST before there is a token (the QR login).
  pub async fn post_anonymous(&self, base: &str, endpoint: &str, body: Value) -> Result<Value> {
    let resp = headers(self.ctx.http.post(url(base, endpoint)))
      .json_text(body.to_string())
      .timeout(API_TIMEOUT)
      .send()
      .await?
      .check()?;
    parse_body(&resp.text())
  }
}

/// What every request body carries: who is calling (observability only).
fn base_info() -> Value {
  let agent = format!("all-the-platforms/{}", env!("CARGO_PKG_VERSION"));
  json!({ "channel_version": agent, "bot_agent": agent })
}

/// Headers of every request; `X-WECHAT-UIN` is a fresh random uint32,
/// base64 of its decimal digits.
fn headers(req: Req<'_>) -> Req<'_> {
  let uin = STANDARD.encode(rand::random::<u32>().to_string());
  req
    .header("content-type", "application/json")
    .header("authorizationtype", "ilink_bot_token")
    .header("x-wechat-uin", uin)
    .header("ilink-app-id", "bot")
    .header("ilink-app-clientversion", "1")
    .no_cookies()
}

fn url(base: &str, endpoint: &str) -> String {
  format!("{}/{endpoint}", base.trim_end_matches('/'))
}

fn parse_body(text: &str) -> Result<Value> {
  if text.trim().is_empty() {
    return Ok(json!({}));
  }
  Ok(serde_json::from_str(text)?)
}

/// `ret` / `errcode` other than 0 as errors; `-14` means log in again.
pub fn check(endpoint: &str, v: Value) -> Result<Value> {
  let ret = v.i64("ret").unwrap_or(0);
  let code = v.i64("errcode").unwrap_or(0);
  if ret == 0 && code == 0 {
    return Ok(v);
  }
  if ret == SESSION_EXPIRED || code == SESSION_EXPIRED {
    return Err(
      Error::auth("the WeChat bot session expired (errcode -14)")
        .with_hint("log in again with `atp wechat login`"),
    );
  }
  let msg = v.str("errmsg").unwrap_or_default();
  Err(Error::upstream(format!(
    "WeChat {endpoint} failed: ret={ret} errcode={code} {msg}"
  )))
}
