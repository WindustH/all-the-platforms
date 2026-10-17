//! Douyin web transport: the device / browser parameters every
//! `/aweme/v1/web/` call carries, visitor cookies, `a_bogus` (and
//! `x-secsdk-web-signature` on protected paths) and the
//! `{status_code, status_msg}` envelope.
//!
//! The query is built and signed here as one string and sent exactly as
//! signed: the signature covers its bytes. [`Api`] owns the context.
//! Writes (`write`) add what the web client's security SDKs add and wait
//! 2 to 6 s first.

mod write;

use std::sync::OnceLock;

use atp_core::http::{Method, Resp, USER_AGENT};
use atp_core::{Ctx, Error, ErrorCode, Http, Result, Value, ValueExt};

use atp_bytedance::abogus::{self, DOUYIN};
use atp_bytedance::query::{encode, query_string};
use atp_bytedance::visitor;

use crate::sign;

pub const WWW: &str = "https://www.douyin.com";
/// Cookies of a logged-in web session.
pub const LOGIN_COOKIES: &[&str] = &["sessionid"];
const LOGIN_HINT: &str = "log in with `atp douyin login` (QR code), `atp douyin login-sms`, \
   `--browser` (a browser where you opened douyin.com) or `--cookie`";
const WALL_HINT: &str = "Douyin refuses this to visitors or flagged clients: log in with \
   `atp douyin login --browser`, retry later, or use a residential network in mainland China";
const CHECK_HINT: &str = "pass the check on douyin.com in your browser (do the same thing \
   there once), then retry later; slow down with --interval";

/// Writes whose answers the web client checks for an identity check.
const DECISION_PATHS: &[&str] = &[
  "/aweme/v1/web/commit/item/digg/",
  "/aweme/v1/web/commit/follow/user/",
  "/aweme/v1/web/comment/publish",
  "/aweme/v1/web/aweme/collect/",
  "/aweme/v1/web/series/collect/",
  "/aweme/v1/web/mix/collect/",
  "/aweme/v1/web/music/collect/",
  "/aweme/v1/web/collects/maintain/",
];

/// `version_code` / `version_name` the web client's request interceptor
/// sends on these paths instead of the common ones.
const VERSIONS: &[(&str, &str, &str)] = &[("/aweme/v1/web/collects/maintain/", "230000", "23.0.0")];

/// The transport: the context with Douyin's referer.
pub struct Api {
  pub ctx: Ctx,
  /// The logged-in account's uid, once looked up.
  pub me: OnceLock<String>,
}

impl Api {
  pub fn new(mut ctx: Ctx) -> Self {
    configure(&mut ctx.http);
    Self {
      ctx,
      me: OnceLock::new(),
    }
  }

  pub fn get(&self, path: &str) -> Call<'_> {
    Call::new(self, path, Method::GET)
  }

  /// A read sent as a form POST (the body is signed too).
  pub fn post(&self, path: &str) -> Call<'_> {
    Call::new(self, path, Method::POST)
  }

  /// Fail with `not_authenticated` unless the session cookie is present.
  pub fn require_login(&self) -> Result<()> {
    self.ctx.require_login(LOGIN_COOKIES)
  }
}

fn configure(http: &mut Http) {
  http.set_header("referer", &format!("{WWW}/"));
}

/// Parameters the web client sends with every call (Chrome on macOS,
/// 1536×864, as `a_bogus` reports it), with the version it names for
/// `path`.
fn common(chrome: &str, path: &str) -> Vec<(&'static str, String)> {
  let (code, name) = VERSIONS
    .iter()
    .find(|(p, _, _)| *p == path)
    .map_or(("290100", "29.1.0"), |(_, code, name)| (*code, *name));
  [
    ("device_platform", "webapp"),
    ("aid", "6383"),
    ("channel", "channel_pc_web"),
    ("update_version_code", "170400"),
    ("pc_client_type", "1"),
    ("pc_libra_divert", "Mac"),
    ("support_h265", "1"),
    ("support_dash", "1"),
    ("version_code", code),
    ("version_name", name),
    ("cookie_enabled", "true"),
    ("screen_width", "1536"),
    ("screen_height", "864"),
    ("browser_language", "zh-CN"),
    ("browser_platform", "MacIntel"),
    ("browser_name", "Chrome"),
    ("browser_version", chrome),
    ("browser_online", "true"),
    ("engine_name", "Blink"),
    ("engine_version", chrome),
    ("os_name", "Mac OS"),
    ("os_version", "10.15.7"),
    ("cpu_core_num", "8"),
    ("device_memory", "8"),
    ("platform", "PC"),
    ("downlink", "10"),
    ("effective_type", "4g"),
    ("round_trip_time", "50"),
  ]
  .into_iter()
  .map(|(k, v)| (k, v.to_owned()))
  .collect()
}

pub struct Call<'a> {
  api: &'a Api,
  path: String,
  method: Method,
  query: Vec<(String, String)>,
  form: Vec<(String, String)>,
  /// A body sent as it is instead of the form (JSON text).
  body: Option<String>,
  /// Parameters after the signature (not signed).
  tail: Vec<(String, String)>,
  headers: Vec<(&'static str, String)>,
  bare: bool,
}

impl<'a> Call<'a> {
  fn new(api: &'a Api, path: &str, method: Method) -> Self {
    Self {
      api,
      path: path.to_owned(),
      method,
      query: Vec::new(),
      form: Vec::new(),
      body: None,
      tail: Vec::new(),
      headers: Vec::new(),
      bare: false,
    }
  }

  /// A header of this call only.
  pub fn header(mut self, name: &'static str, value: impl ToString) -> Self {
    self.headers.push((name, value.to_string()));
    self
  }

  /// A query parameter after the signature, which does not cover it.
  pub fn tail(mut self, key: &str, value: impl ToString) -> Self {
    self.tail.push((key.to_owned(), value.to_string()));
    self
  }

  /// Query parameter; later ones replace common parameters of the same name.
  pub fn arg(mut self, key: &str, value: impl ToString) -> Self {
    let value = value.to_string();
    match self.query.iter_mut().find(|(k, _)| k == key) {
      Some(slot) => slot.1 = value,
      None => self.query.push((key.to_owned(), value)),
    }
    self
  }

  /// A query parameter given once per value (`item_ids=1&item_ids=2`),
  /// as the web client sends lists.
  pub fn args<T: ToString>(mut self, key: &str, values: impl IntoIterator<Item = T>) -> Self {
    for value in values {
      self.query.push((key.to_owned(), value.to_string()));
    }
    self
  }

  /// Form field of a POST body.
  pub fn field(mut self, key: &str, value: impl ToString) -> Self {
    self.form.push((key.to_owned(), value.to_string()));
    self
  }

  /// A JSON body instead of the form. The web client sends it as text
  /// under the form's content type, and so does this.
  pub fn json(mut self, value: &Value) -> Self {
    self.body = Some(value.to_string());
    self
  }

  /// Without the common device parameters (endpoints outside `/aweme/v1/web/`).
  pub fn bare(mut self) -> Self {
    self.bare = true;
    self
  }

  /// The final query string: common parameters, the call's own, the visitor
  /// ids, `a_bogus`, and on protected paths the web signature.
  fn signed_query(&self, body: &str) -> String {
    let http = &self.api.ctx.http;
    let chrome = format!("{}.0.0.0", atp_core::http::CHROME_VERSION);
    let mut pairs: Vec<(String, String)> = if self.bare {
      Vec::new()
    } else {
      common(&chrome, &self.path)
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
    };
    // The call's parameters replace common ones of the same name; its own
    // repeated keys all stay.
    let common_len = pairs.len();
    for (k, v) in &self.query {
      match pairs[..common_len].iter_mut().find(|(pk, _)| pk == k) {
        Some(slot) => slot.1 = v.clone(),
        None => pairs.push((k.clone(), v.clone())),
      }
    }
    let uifid = http.cookie("UIFID").filter(|u| !u.is_empty());
    if !self.bare {
      if let Some(fp) = http.cookie("s_v_web_id") {
        pairs.push(("verifyFp".into(), fp.clone()));
        pairs.push(("fp".into(), fp));
      }
      if let Some(u) = &uifid {
        pairs.push(("uifid".into(), u.clone()));
      }
      pairs.push(("msToken".into(), http.cookie("msToken").unwrap_or_default()));
    }
    let query = query_string(&pairs);
    let now = jiff::Timestamp::now().as_millisecond();
    let a_bogus = abogus::sign(DOUYIN, &query, body, USER_AGENT, now);
    let signed = format!("{query}&a_bogus={}", encode(&a_bogus));
    let signed = match uifid {
      Some(u) if sign::is_protected(&self.path) => sign::web_sign(&signed, &u, now / 1000),
      _ => signed,
    };
    if self.tail.is_empty() {
      return signed;
    }
    format!("{signed}&{}", query_string(&self.tail))
  }

  /// Send and return the JSON body after checking `status_code`.
  pub async fn send(self) -> Result<Value> {
    let v = self.send_raw().await?;
    check(v)
  }

  /// Send and return the JSON body whatever its `status_code`.
  pub async fn send_raw(self) -> Result<Value> {
    let api = self.api;
    let ctx = &api.ctx;
    visitor::ensure(&api.ctx).await;
    let form = self.method == Method::POST;
    let body = match &self.body {
      Some(body) => body.clone(),
      None => query_string(&self.form),
    };
    let url = format!(
      "{}{}?{}",
      host(&self.path),
      self.path,
      self.signed_query(if form { &body } else { "" })
    );
    let mut req = ctx.http.request(self.method.clone(), url);
    // Browsers send the visitor id as a header too.
    if let Some(uifid) = ctx.http.cookie("UIFID") {
      req = req.header("uifid", uifid);
    }
    for (name, value) in &self.headers {
      req = req.header(name, value);
    }
    if form {
      req = req.header("origin", WWW).bytes(
        body.into_bytes(),
        "application/x-www-form-urlencoded; charset=UTF-8",
      );
    }
    let resp = req.send().await?;
    tracing::debug!(
      "{}: {}",
      self.path,
      atp_core::text::truncate(&resp.text(), 300)
    );
    checks(&resp, &self.path)?;
    if resp.body.iter().all(u8::is_ascii_whitespace) {
      return Err(Error::new(ErrorCode::VerificationRequired, "empty answer").with_hint(WALL_HINT));
    }
    if resp.text().contains("Uifid Not Found") {
      return Err(
        Error::auth("Douyin serves this only to browser sessions (no UIFID cookie)")
          .with_hint(LOGIN_HINT),
      );
    }
    if !resp.status.is_success() {
      return Err(atp_core::http::status_error(resp.status, &resp.text()).with_hint(WALL_HINT));
    }
    resp.value().map_err(|_| {
      Error::new(
        ErrorCode::VerificationRequired,
        "Douyin answered with a page instead of data",
      )
      .with_hint(WALL_HINT)
    })
  }
}

/// The human checks an answer announces in its headers, as the web
/// client's response interceptor reads them: a captcha
/// (`x-vc-bdturing-parameters`) on any call, an identity check
/// (`x-tt-verify-passport-decision`) on the writes it lists
/// ([`DECISION_PATHS`]).
fn checks(resp: &Resp, path: &str) -> Result<()> {
  let logid = resp.header("x-tt-logid").unwrap_or_default();
  let decision = resp.header("x-tt-verify-passport-decision").is_some();
  let what = if resp.header("x-vc-bdturing-parameters").is_some() {
    "a captcha"
  } else if decision && DECISION_PATHS.contains(&path) {
    "an identity check (二次验证)"
  } else {
    return Ok(());
  };
  Err(
    Error::new(
      ErrorCode::VerificationRequired,
      format!("Douyin wants {what} before this call (log id {logid})"),
    )
    .with_url(format!("{WWW}/"))
    .with_hint(CHECK_HINT),
  )
}

fn host(path: &str) -> &'static str {
  if path.starts_with("/webcast/") {
    "https://live.douyin.com"
  } else if path.starts_with("/web/api/v2/") {
    "https://www.iesdouyin.com"
  } else {
    WWW
  }
}

/// Map the envelope's `status_code` to an error.
pub fn check(v: Value) -> Result<Value> {
  let code = v.i64("status_code").unwrap_or(0);
  if code == 0 {
    return Ok(v);
  }
  let msg = v
    .first_str(&["status_msg", "status_message", "message"])
    .unwrap_or_else(|| format!("status_code {code}"));
  Err(match code {
    8 => Error::auth(format!("not logged in: {msg}")).with_hint(LOGIN_HINT),
    _ => Error::upstream(format!("Douyin: {msg} ({code})")),
  })
}
