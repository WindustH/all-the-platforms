//! QQ Music transport: module calls on the `u.y.qq.com` gateway, signed
//! as the web client signs them ([`sign::zzc`]), several in one request
//! when a page needs them together; and the JSON pages of `c.y.qq.com`
//! that profiles come from.
//!
//! Gateway calls carry QQ Music's app as their user agent and say they
//! come from the web player (`ct` 24), or from the desktop player (`ct`
//! 19) for search, which then answers without a login. Logins and the
//! `c.y.qq.com` pages go as the browser.

use std::time::Duration;

use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};
use serde_json::Map;

use crate::sign;

pub const WEB: &str = "https://y.qq.com";
const GATEWAY: &str = "https://u.y.qq.com/cgi-bin/musics.fcg";
const PAGES: &str = "https://c.y.qq.com";

/// Session cookie of a logged-in account (the music key).
pub const LOGIN_COOKIES: &[&str] = &["qqmusic_key"];
/// The cookies a QQ Music session consists of. Logins keep only these:
/// the QQ account's own cookies (`skey`, `p_skey` …) are not needed.
pub const SESSION_COOKIES: &[&str] = &[
  "uin",
  "qqmusic_uin",
  "p_uin",
  "wxuin",
  "euin",
  "qqmusic_key",
  "qm_keyst",
  "tmeLoginType",
  "login_type",
  "wxopenid",
  "wxunionid",
  "wxrefresh_token",
  "psrf_qqopenid",
  "psrf_qqunionid",
  "psrf_qqaccess_token",
  "psrf_qqrefresh_token",
  "psrf_access_token_expiresAt",
  "psrf_musickey_createtime",
];

/// The user agent of gateway calls: QQ Music's app. With a browser's, the
/// gateway wants a login or a check (2001) before it searches.
const CLIENT_UA: &str = "QQMusic 14090508(android 12)";

/// Which player a gateway call says it comes from (`ct`, `cv`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Client {
  /// y.qq.com: full answers (reply counts, comment sequence numbers).
  Web,
  /// The desktop player: searches without a login.
  Desktop,
}

impl Client {
  fn version(self) -> (u64, u64) {
    match self {
      Client::Web => (24, 4_747_474),
      Client::Desktop => (19, 2201),
    }
  }
}

/// Login type of the gateway's `comm`: QQ, or WeChat when the session has
/// a WeChat open id.
const LOGIN_QQ: u64 = 2;
const LOGIN_WECHAT: u64 = 1;

/// The transport: the context with the web player's headers.
pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("referer", "https://y.qq.com/");
    Self { ctx }
  }

  /// One call of a gateway module.
  pub fn call(&self, module: &str, method: &str) -> Call<'_> {
    Call {
      api: self,
      module: module.to_owned(),
      method: method.to_owned(),
      param: Map::new(),
      write: false,
      client: Client::Web,
    }
  }

  /// Several calls in one request; their `data` in the same order.
  pub async fn batch(&self, calls: Vec<Call<'_>>) -> Result<Vec<Value>> {
    if calls.iter().any(|c| c.write) {
      self.paced_write().await?;
    }
    let client = if calls.iter().any(|c| c.client == Client::Desktop) {
      Client::Desktop
    } else {
      Client::Web
    };
    let mut body = Map::new();
    body.insert("comm".into(), self.comm(None, client));
    for (i, call) in calls.iter().enumerate() {
      body.insert(format!("req_{i}"), call.request());
    }
    let v = self.post(&Value::Object(body)).await?;
    (0..calls.len())
      .map(|i| self.data(v.at(&format!("req_{i}"))))
      .collect()
  }

  /// Fail with `not_authenticated` unless the session has a music key.
  pub fn require_login(&self) -> Result<()> {
    if self.key().is_some() {
      return Ok(());
    }
    self.ctx.require_login(LOGIN_COOKIES)
  }

  /// The account number (`musicid`) of the session: the WeChat or QQ
  /// `uin` cookie without its `o` and leading zeros.
  pub fn uin(&self) -> Option<u64> {
    let http = &self.ctx.http;
    let first = if self.wechat() { "wxuin" } else { "uin" };
    [first, "p_uin", "qqmusic_uin"]
      .iter()
      .filter_map(|c| http.cookie(c))
      .find_map(|v| v.trim_start_matches('o').parse().ok())
      .filter(|n| *n > 10_000)
  }

  /// The music key of the session.
  pub fn key(&self) -> Option<String> {
    let http = &self.ctx.http;
    http
      .cookie("qqmusic_key")
      .or_else(|| http.cookie("qm_keyst"))
  }

  fn wechat(&self) -> bool {
    self.ctx.http.has_cookie("wxopenid")
  }

  /// `g_tk`: the hash of the music key (5381 without one).
  pub fn g_tk(&self) -> u32 {
    sign::hash33(&self.key().unwrap_or_default(), 5381)
  }

  /// The `comm` of a request: the player it comes from and the session's
  /// account, or no account and this `login_type` for a login call.
  fn comm(&self, login_type: Option<u64>, client: Client) -> Value {
    let account = match login_type {
      Some(_) => None,
      None => self.key().and_then(|_| self.uin()),
    };
    let (uin, g_tk) = match account {
      Some(uin) => (uin, self.g_tk()),
      None => (0, sign::hash33("", 5381)),
    };
    let (ct, cv) = client.version();
    let mut comm = json!({"ct": ct, "cv": cv, "chid": "0", "uin": uin.to_string(), "g_tk": g_tk});
    if client == Client::Web {
      for (key, value) in [
        ("platform", json!("yqq.json")),
        ("g_tk_new_20200303", json!(g_tk)),
        ("format", json!("json")),
        ("inCharset", json!("utf-8")),
        ("outCharset", json!("utf-8")),
        ("notice", json!(0)),
        ("needNewCode", json!(1)),
      ] {
        comm[key] = value;
      }
    }
    if let Some(t) = login_type.or_else(|| account.map(|_| self.login_type())) {
      comm["tmeLoginType"] = json!(t);
    }
    comm
  }

  /// The session's `tmeLoginType`: its cookie, or QQ unless the session
  /// has a WeChat open id.
  fn login_type(&self) -> u64 {
    let cookie = self.ctx.http.cookie("tmeLoginType");
    match cookie.and_then(|t| t.parse().ok()) {
      Some(t) => t,
      None if self.wechat() => LOGIN_WECHAT,
      None => LOGIN_QQ,
    }
  }

  /// Send a signed body; the whole answer once its top `code` is 0.
  async fn post(&self, body: &Value) -> Result<Value> {
    let text = body.to_string();
    let now = jiff::Timestamp::now().as_millisecond();
    let v = self
      .ctx
      .http
      .post(GATEWAY)
      .header("origin", WEB)
      .header("user-agent", CLIENT_UA)
      .query("_", now)
      .query("sign", sign::zzc(&text))
      .json_text(text)
      .send()
      .await?
      .check()?
      .value()?;
    match v.i64("code") {
      None | Some(0) => Ok(v),
      Some(code) => Err(self.error(code, &v)),
    }
  }

  /// The `data` of one module's answer, or the error its `code` stands for.
  fn data(&self, answer: &Value) -> Result<Value> {
    match answer.i64("code") {
      Some(0) => Ok(answer.at("data").clone()),
      Some(code) => Err(self.error(code, answer)),
      None => Err(Error::upstream("QQ Music sent no answer for a call")),
    }
  }

  /// A login check and the pause a person takes before a change.
  async fn paced_write(&self) -> Result<()> {
    self.require_login()?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(500), Duration::from_millis(1500))
      .await;
    Ok(())
  }

  /// A JSON page of `c.y.qq.com` (`path` from its root), with the
  /// session's account and `g_tk`; the answer once its `code` is 0.
  pub async fn page(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let uin = self.uin().unwrap_or(0);
    let v = self
      .ctx
      .http
      .get(format!("{PAGES}{path}"))
      .queries([
        ("format", "json".to_owned()),
        ("inCharset", "utf-8".to_owned()),
        ("outCharset", "utf-8".to_owned()),
        ("g_tk", self.g_tk().to_string()),
        ("loginUin", uin.to_string()),
        ("hostUin", "0".to_owned()),
        ("needNewCode", "0".to_owned()),
      ])
      .queries(query.iter().map(|(k, v)| (*k, v.clone())))
      .send()
      .await?
      .check()?
      .value()?;
    match v.i64("code") {
      None | Some(0) => Ok(v),
      Some(code) => Err(self.error(code, &v)),
    }
  }

  /// The error an answer's `code` stands for.
  pub fn error(&self, code: i64, v: &Value) -> Error {
    error(&self.ctx.login_hint(), code, v)
  }
}

/// One module call under construction.
pub struct Call<'a> {
  api: &'a Api,
  module: String,
  method: String,
  param: Map<String, Value>,
  write: bool,
  client: Client,
}

impl Call<'_> {
  /// A parameter of the call.
  pub fn arg(mut self, key: &str, value: impl Into<Value>) -> Self {
    self.param.insert(key.to_owned(), value.into());
    self
  }

  /// A change to the account: needs a login and is paced like a person.
  pub fn write(mut self) -> Self {
    self.write = true;
    self
  }

  /// Say the call comes from the desktop player (search).
  pub fn desktop(mut self) -> Self {
    self.client = Client::Desktop;
    self
  }

  fn request(&self) -> Value {
    json!({"module": self.module, "method": self.method, "param": self.param})
  }

  /// Send alone and return the module's `data`.
  pub async fn send(self) -> Result<Value> {
    let api = self.api;
    let mut data = api.batch(vec![self]).await?;
    Ok(data.pop().unwrap_or_default())
  }

  /// Send alone with this `tmeLoginType` (logins, before the session has
  /// one) and return the module's `data`.
  pub async fn send_as(self, login_type: u64) -> Result<Value> {
    let api = self.api;
    let body = json!({"comm": api.comm(Some(login_type), self.client), "req_0": self.request()});
    let v = api.post(&body).await?;
    api.data(v.at("req_0"))
  }
}

fn error(login_hint: &str, code: i64, v: &Value) -> Error {
  let message = v
    .first_str(&[
      "msg",
      "message",
      "data.Msg",
      "data.msg",
      "data.message",
      "data.errMsg",
    ])
    .unwrap_or_else(|| "request refused".into());
  let text = format!("QQ Music: {message} ({code})");
  match code {
    1000 | 104400 | 104401 => Error::auth(text).with_hint(login_hint),
    2001 => verification(text, v),
    2000 => Error::upstream(format!("QQ Music refused the request signature ({code})")),
    104604 => Error::new(ErrorCode::RateLimited, text),
    20277 | 20278 | 20450 => Error::new(ErrorCode::PermissionDenied, text)
      .with_hint("QQ Music restricts this account; see the app"),
    20279 => Error::new(ErrorCode::PermissionDenied, text)
      .with_hint("too many devices are logged in; log out of one in the app"),
    _ => Error::upstream(text),
  }
}

/// A risk check: `verification_required`, with the page that clears it
/// when the answer names one.
fn verification(text: String, v: &Value) -> Error {
  let error = Error::new(ErrorCode::VerificationRequired, text).with_hint(
    "QQ Music wants a human check: open the page, or log in, or pass it in the app, then retry",
  );
  let page = v
    .first_str(&[
      "feedbackURL",
      "data.feedbackURL",
      "data.safetyUrl",
      "data.url",
      "url",
    ])
    .filter(|u| u.starts_with("http"));
  match page {
    Some(url) => error.with_url(url),
    None => error,
  }
}
