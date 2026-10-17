//! creator.douyin.com (抖音创作者中心), the backend of the web uploader:
//! the parameters its pages add to every call, `a_bogus` with the creator
//! page's own ids, the secsdk CSRF token, the ticket-guard headers the
//! page's secure consumer SDK adds on the paths it guards, and the
//! answers that stand for a human check.
//!
//! Read from the page's own bundles (the `douyin_creator_content` micro
//! app, `secure-consumer-sdk` 1.0.0.22, `sdk-glue` 1.0.0.62 with `bdms`
//! 1.0.1.16, secsdk 1.2.22). The page sends no `x-tt-session-dtrait`.

use std::time::Duration;

use atp_bytedance::abogus::{self, DOUYIN_CREATOR};
use atp_bytedance::query::{encode, query_string};
use atp_bytedance::visitor;
use atp_core::http::{Method, Resp, USER_AGENT, status_error};
use atp_core::{Error, ErrorCode, Result, Value, ValueExt};

use crate::api::Api;
use crate::sign::guard::Guard;

pub const CREATOR: &str = "https://creator.douyin.com";
/// The upload page: the referer of its calls and the page of human checks.
pub const UPLOAD_PAGE: &str = "https://creator.douyin.com/creator-micro/content/upload";
/// Paths the page's secure consumer SDK signs with the ticket-guard key.
const GUARDED: &[&str] = &[
  "/web/api/media/aweme/create_v2/",
  "/web/api/media/aweme/delete/",
];
/// The secsdk CSRF token comes on a `HEAD` of this path (`TOKEN_PATH`).
const CSRF_PATH: &str = "/web/api/media/anchor/search";
/// The passport's clock, read by the consumer SDK before signing.
const TRUSTED_TIME: &str = "/passport/user_info/get_sec_ts/";
const LOGIN_HINT: &str = "log in again with `atp douyin login` (QR code) or `atp douyin login-sms`";
const CHECK_HINT: &str = "open creator.douyin.com in your browser, publish something there once \
   (passing any identity check it asks for), then run the command again; or retry later";

/// The creator platform for one command.
pub struct Creator<'a> {
  pub api: &'a Api,
  csrf: Option<String>,
}

impl<'a> Creator<'a> {
  /// Open it for this command: a logged-in session and its CSRF token.
  pub async fn open(api: &'a Api) -> Result<Self> {
    api.require_login()?;
    visitor::ensure(&api.ctx).await;
    let csrf = csrf_token(api).await;
    if csrf.is_none() {
      tracing::debug!("creator.douyin.com handed out no secsdk CSRF token");
    }
    Ok(Self { api, csrf })
  }

  pub fn get(&self, path: &str) -> Call<'_> {
    Call::new(self, Method::GET, path, None)
  }

  /// A POST with a JSON body, as the page's axios client sends it.
  pub fn post_json(&self, path: &str, body: &Value) -> Call<'_> {
    let body = (body.to_string().into_bytes(), "application/json");
    Call::new(self, Method::POST, path, Some(body))
  }

  /// A POST with a form body (`qs.stringify`).
  pub fn post_form(&self, path: &str, form: &[(String, String)]) -> Call<'_> {
    let body = (
      query_string(form).into_bytes(),
      "application/x-www-form-urlencoded;charset=UTF-8",
    );
    Call::new(self, Method::POST, path, Some(body))
  }

  /// The ticket-guard headers of a guarded write to `path`.
  async fn guard_headers(&self, path: &str) -> Result<Vec<(&'static str, String)>> {
    let guard = Guard::load(&self.api.ctx)?;
    let (timestamp, trusted) = match trusted_time(self.api).await {
      Some(ts) => (ts, true),
      None => (jiff::Timestamp::now().as_second(), false),
    };
    guard.consumer_headers(path, timestamp, trusted)
  }
}

/// One call to creator.douyin.com.
pub struct Call<'c> {
  creator: &'c Creator<'c>,
  method: Method,
  path: String,
  query: Vec<(String, String)>,
  body: Option<(Vec<u8>, &'static str)>,
  referer: String,
}

impl<'c> Call<'c> {
  fn new(
    creator: &'c Creator<'c>,
    method: Method,
    path: &str,
    body: Option<(Vec<u8>, &'static str)>,
  ) -> Self {
    Self {
      creator,
      method,
      path: path.to_owned(),
      query: Vec::new(),
      body,
      referer: UPLOAD_PAGE.to_owned(),
    }
  }

  /// A query parameter, before the page's own.
  pub fn arg(mut self, key: &str, value: impl ToString) -> Self {
    self.query.push((key.to_owned(), value.to_string()));
    self
  }

  /// The page the call comes from.
  pub fn referer(mut self, page: &str) -> Self {
    self.referer = page.to_owned();
    self
  }

  /// The query as signed: the call's parameters, the page's, `msToken`
  /// and `a_bogus` over the query and the body.
  fn signed_query(&self) -> String {
    let http = &self.creator.api.ctx.http;
    let mut pairs = self.query.clone();
    for (key, value) in page_params() {
      if !pairs.iter().any(|(k, _)| k == key) {
        pairs.push((key.to_owned(), value));
      }
    }
    pairs.push(("msToken".into(), http.cookie("msToken").unwrap_or_default()));
    let query = query_string(&pairs);
    let body = self
      .body
      .as_ref()
      .map(|(b, _)| String::from_utf8_lossy(b).into_owned())
      .unwrap_or_default();
    let now = jiff::Timestamp::now().as_millisecond();
    let a_bogus = abogus::sign(DOUYIN_CREATOR, &query, &body, USER_AGENT, now);
    format!("{query}&a_bogus={}", encode(&a_bogus))
  }

  /// Send and return the JSON answer after checking it.
  pub async fn send(self) -> Result<Value> {
    let creator = self.creator;
    let http = &creator.api.ctx.http;
    let url = format!("{CREATOR}{}?{}", self.path, self.signed_query());
    let mut req = http
      .request(self.method.clone(), url)
      .header("accept", "application/json, text/plain, */*")
      .header("referer", &self.referer);
    if let Some(token) = &creator.csrf {
      req = req.header("x-secsdk-csrf-token", token);
    }
    if let Some(token) = http.cookie("csrf_token") {
      req = req.header("x-csrf-token", token);
    }
    if GUARDED.contains(&self.path.as_str()) {
      for (name, value) in creator.guard_headers(&self.path).await? {
        req = req.header(name, value);
      }
    }
    if let Some((body, content_type)) = self.body {
      req = req.header("origin", CREATOR).bytes(body, content_type);
    }
    let resp = req.send().await?;
    // Only the envelope: answers carry upload keys.
    let head: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    tracing::debug!(
      "{}: HTTP {}, {} bytes, status_code {}, {}",
      self.path,
      resp.status.as_u16(),
      resp.body.len(),
      head.str("status_code").unwrap_or_default(),
      head.str("status_msg").unwrap_or_default()
    );
    check(&resp)
  }
}

/// What the page's request interceptor adds to every query (Chrome on
/// macOS, 1536×864, as `a_bogus` reports it); `aid` 1128 unless the call
/// names its own.
fn page_params() -> Vec<(&'static str, String)> {
  let app_version = USER_AGENT.trim_start_matches("Mozilla/");
  [
    ("cookie_enabled", "true"),
    ("screen_width", "1536"),
    ("screen_height", "864"),
    ("browser_language", "zh-CN"),
    ("browser_platform", "MacIntel"),
    ("browser_name", "Mozilla"),
    ("browser_version", app_version),
    ("browser_online", "true"),
    ("timezone_name", "Asia/Shanghai"),
    ("aid", "1128"),
    ("support_h265", "1"),
  ]
  .into_iter()
  .map(|(k, v)| (k, v.to_owned()))
  .collect()
}

/// The secsdk CSRF token (`x-ware-csrf-token: 0,<token>,…`).
async fn csrf_token(api: &Api) -> Option<String> {
  let resp = api
    .ctx
    .http
    .request(Method::HEAD, format!("{CREATOR}{CSRF_PATH}"))
    .header("referer", UPLOAD_PAGE)
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

/// The passport's time (`bd-ticket-guard-ts`), as the consumer SDK reads
/// it; `None` when it does not answer, and the SDK then signs with the
/// local clock.
async fn trusted_time(api: &Api) -> Option<i64> {
  let form = "aid=2906&is_from_ttaccountsdk=1";
  let resp = api
    .ctx
    .http
    .post(format!("{CREATOR}{TRUSTED_TIME}?{form}"))
    .header("referer", UPLOAD_PAGE)
    .header("origin", CREATOR)
    .header("accept", "application/json")
    .bytes(
      form.as_bytes().to_vec(),
      "application/x-www-form-urlencoded",
    )
    .timeout(Duration::from_secs(10))
    .send()
    .await
    .ok()?;
  resp
    .header("bd-ticket-guard-ts")
    .and_then(|ts| ts.trim().parse().ok())
    .filter(|ts: &i64| *ts > 0)
}

/// Map an answer of creator.douyin.com: the human checks it announces in
/// headers, empty answers, pages, then the `status_code` envelope.
pub fn check(resp: &Resp) -> Result<Value> {
  let logid = resp.header("x-tt-logid").unwrap_or_default().to_owned();
  if let Some(decision) = resp.header("x-tt-verify-passport-decision") {
    let scene = serde_json::from_str::<Value>(decision)
      .ok()
      .and_then(|v| v.str("event_params.verify_scene"))
      .unwrap_or_default();
    return Err(check_error(format!(
      "Douyin wants an identity check (二次验证, scene `{scene}`) before this write (log id {logid})"
    )));
  }
  if resp.header("x-vc-bdturing-parameters").is_some() {
    return Err(check_error(format!(
      "Douyin wants a captcha before this call (log id {logid})"
    )));
  }
  if resp.body.iter().all(u8::is_ascii_whitespace) {
    return Err(check_error(format!(
      "creator.douyin.com answered HTTP {} with nothing: risk control, or a signature it \
       did not accept (log id {logid})",
      resp.status.as_u16()
    )));
  }
  if !resp.status.is_success() {
    return Err(status_error(resp.status, &resp.text()));
  }
  let v = resp
    .value()
    .map_err(|_| check_error("creator.douyin.com answered with a page instead of data".into()))?;
  envelope(v)
}

fn check_error(message: String) -> Error {
  Error::new(ErrorCode::VerificationRequired, message)
    .with_url(UPLOAD_PAGE)
    .with_hint(CHECK_HINT)
    .unclearable()
}

/// The `{status_code, status_msg}` envelope.
fn envelope(v: Value) -> Result<Value> {
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
