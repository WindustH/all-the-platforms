//! Juejin transport: the JSON API of `api.juejin.cn` the site calls. Every
//! call carries the site's app id (`aid=2608`), a visitor id (`uuid`, kept
//! for a month) and `spider=0`. Reads need no signature. Some writes are
//! among the paths Juejin's page signs with ByteDance's `a_bogus` (its
//! `bdms` SDK): those go out signed with [`atp_bytedance::abogus`] and an
//! `msToken`, as the page sends them. Answers are `{err_no, err_msg,
//! data}`; a page instead of JSON is Juejin's bot check.

use std::time::Duration;

use atp_bytedance::abogus::{self, JUEJIN};
use atp_bytedance::query::{encode, query_string};
use atp_bytedance::visitor;
use atp_core::http::{Method, Resp, USER_AGENT};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};
use rand::Rng;

pub const API: &str = "https://api.juejin.cn";
pub const SITE: &str = "https://juejin.cn";
pub const AID: u64 = 2608;
/// Cookies of a logged-in session.
pub const LOGIN_COOKIES: &[&str] = &["sessionid"];
/// The paths the page signs (from its `bdms` options) that ATP writes to.
const SIGNED: &[&str] = &[
  "/growth_api/v1/check_in",
  "/interact_api/v1/digg/save",
  "/interact_api/v1/comment/publish",
  "/interact_api/v1/reply/publish",
];
const UUID_KEY: &str = "uuid";
const UUID_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
const LOGIN_HINT: &str = "log in with `atp juejin login --browser` or `--cookie`";

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("referer", &format!("{SITE}/"));
    ctx.http.set_header("origin", SITE);
    Self { ctx }
  }

  pub fn require_login(&self) -> Result<()> {
    self
      .ctx
      .require_login(LOGIN_COOKIES)
      .map_err(|e| e.with_hint(LOGIN_HINT))
  }

  /// The visitor id the site keeps (`user_unique_id`): nineteen digits.
  fn uuid(&self) -> String {
    let store = &self.ctx.store;
    if let Some(id) = store.cache_get::<String>(UUID_KEY, UUID_TTL) {
      return id;
    }
    let mut rng = rand::rng();
    let id: String = std::iter::once(rng.random_range(1..=9u8))
      .chain((0..18).map(|_| rng.random_range(0..=9u8)))
      .map(|d| char::from(b'0' + d))
      .collect();
    store.cache_put(UUID_KEY, &id);
    id
  }

  fn common(&self) -> Vec<(String, String)> {
    vec![
      ("aid".into(), AID.to_string()),
      ("uuid".into(), self.uuid()),
      ("spider".into(), "0".into()),
    ]
  }

  pub async fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
    let resp = self
      .ctx
      .http
      .get(format!("{API}{path}"))
      .queries(params.iter().map(|(k, v)| (*k, v)))
      .queries(self.common())
      .send()
      .await?;
    self.check(resp, path)
  }

  /// `POST` of a JSON body.
  pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
    let resp = self
      .ctx
      .http
      .request(Method::POST, format!("{API}{path}"))
      .queries(self.common())
      .json(body)
      .send()
      .await?;
    self.check(resp, path)
  }

  /// A change to the account: needs a login, waits a moment like a person
  /// would, and is signed when the page signs that path.
  pub async fn write(&self, path: &str, body: &Value) -> Result<Value> {
    self.require_login()?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(800), Duration::from_millis(2000))
      .await;
    if !SIGNED.contains(&path) {
      return self.post(path, body).await;
    }
    let http = &self.ctx.http;
    let mut pairs = self.common();
    pairs.push((
      "msToken".into(),
      http.cookie("msToken").unwrap_or_else(visitor::ms_token),
    ));
    let query = query_string(&pairs);
    let text = serde_json::to_string(body)?;
    let now = jiff::Timestamp::now().as_millisecond();
    let a_bogus = abogus::sign(JUEJIN, &query, &text, USER_AGENT, now);
    let url = format!("{API}{path}?{query}&a_bogus={}", encode(&a_bogus));
    let resp = http
      .request(Method::POST, url)
      .json_text(text)
      .send()
      .await?;
    self.check(resp, path)
  }

  /// The answer's data, or the error it stands for.
  fn check(&self, resp: Resp, path: &str) -> Result<Value> {
    if let Some(token) = resp.header("x-ms-token").filter(|t| !t.is_empty()) {
      self.ctx.http.set_cookie("msToken", token);
    }
    let Ok(v) = serde_json::from_slice::<Value>(&resp.body) else {
      if !resp.status.is_success() && resp.status.as_u16() != 403 {
        return Err(atp_core::http::status_error(resp.status, &resp.text()));
      }
      return Err(
        Error::new(
          ErrorCode::VerificationRequired,
          format!("Juejin answered {path} with its bot check instead of data"),
        )
        .with_url(format!("{SITE}/"))
        .with_hint("pass the check at juejin.cn in a browser, then retry"),
      );
    };
    match v.i64("err_no") {
      Some(0) | None => Ok(v),
      Some(code) => Err(error(code, &v)),
    }
  }
}

/// Juejin's `err_no` as a shared error.
fn error(code: i64, v: &Value) -> Error {
  let message = v.str("err_msg").unwrap_or_default();
  let text = format!("Juejin: {message} ({code})");
  let has = |words: &[&str]| words.iter().any(|w| message.contains(w));
  if code == 403 || has(&["must login", "未登录", "登录"]) {
    return Error::auth(text).with_hint(LOGIN_HINT);
  }
  if has(&["验证", "verify", "captcha", "风险", "异常"]) {
    return Error::new(ErrorCode::VerificationRequired, text)
      .with_url(format!("{SITE}/"))
      .with_hint("pass the check at juejin.cn in a browser, then retry");
  }
  if has(&["频繁", "too many", "limit"]) {
    return Error::new(ErrorCode::RateLimited, text);
  }
  if code == 404 || has(&["不存在", "not exist", "not found"]) {
    return Error::not_found(text);
  }
  if has(&["参数", "param"]) {
    return Error::input(text);
  }
  Error::upstream(text)
}
