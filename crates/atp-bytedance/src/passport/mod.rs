//! Logins through a ByteDance SSO passport. QR codes (`/get_qrcode/` then
//! `/check_qrconnect/`, as `sso.toutiao.com` serves them) are scanned with
//! the site's app; a confirmed scan hands back a redirect that sets the
//! session cookies. Phone logins (`/send_activation_code/v2/` then
//! `/quick_login/v2/`) text a code that the person types in, as
//! `cv-cat/catbus` (`src/platforms/douyin/web/passport.ts`, MIT) records
//! them on login.douyin.com. [`web`] is the account web SDK's passport
//! (`/passport/web/…`), which www.douyin.com uses for QR codes.

pub mod web;

use atp_core::http::{Http, Resp};
use atp_core::{Error, ErrorCode, Result, Value, ValueExt, json};

use crate::ticket_guard::SERVER_DATA;

/// One SSO passport: its host, the service it logs into and the app id.
#[derive(Debug, Clone, Copy)]
pub struct Sso {
  pub host: &'static str,
  pub service: &'static str,
  pub aid: u64,
}

pub const TOUTIAO: Sso = Sso {
  host: "https://sso.toutiao.com",
  service: "https://www.toutiao.com",
  aid: 24,
};

/// Douyin's passport host, for phone logins.
pub const DOUYIN: Sso = Sso {
  host: "https://login.douyin.com",
  service: "https://www.douyin.com",
  aid: 6383,
};

/// A started QR login: what the QR code encodes, and its token for polling.
pub struct Qr {
  pub url: String,
  pub token: String,
}

pub enum QrState {
  Waiting,
  Scanned,
  /// Confirmed on the phone; the redirect still has to be followed.
  Confirmed(String),
  Expired,
}

pub async fn qr_start(http: &Http, sso: Sso) -> Result<Qr> {
  let v = call(http, sso, "/get_qrcode/", None).await?;
  let data = v.at("data");
  match (data.str("qrcode_index_url"), data.str("token")) {
    (Some(url), Some(token)) => Ok(Qr { url, token }),
    _ => Err(failure(&v, "the passport issued no QR code")),
  }
}

pub async fn qr_poll(http: &Http, sso: Sso, token: &str) -> Result<QrState> {
  let v = call(http, sso, "/check_qrconnect/", Some(token)).await?;
  qr_state(&v)
}

/// The state a `check_qrconnect` answer reports.
fn qr_state(v: &Value) -> Result<QrState> {
  let data = v.at("data");
  // 7: polled too often; keep waiting.
  if data.i64("error_code") == Some(7) {
    return Ok(QrState::Waiting);
  }
  // Newer passports answer words, older ones numbers.
  match data.str("status").as_deref() {
    Some("new" | "1") => Ok(QrState::Waiting),
    Some("scanned" | "2") => Ok(QrState::Scanned),
    Some("confirmed" | "3") => match data.str("redirect_url") {
      Some(url) => Ok(QrState::Confirmed(url)),
      None => Err(Error::upstream(
        "the passport confirmed the scan without a redirect",
      )),
    },
    Some("expired" | "5") => Ok(QrState::Expired),
    _ => Err(failure(v, "unexpected QR state")),
  }
}

/// Follow the confirmation redirect so the session cookies land in the jar.
pub async fn finish(http: &Http, redirect: &str) -> Result<Resp> {
  http
    .get(redirect)
    .header("accept", "text/html")
    .send()
    .await
}

async fn call(http: &Http, sso: Sso, path: &str, token: Option<&str>) -> Result<Value> {
  let mut req = http
    .get(format!("{}{path}", sso.host))
    .query("service", sso.service)
    .query("need_logo", "false")
    .query("aid", sso.aid)
    .query("account_sdk_source", "sso")
    .query("language", "zh");
  if let Some(token) = token {
    req = req.query("token", token);
  }
  let resp = req
    .header("referer", format!("{}/", sso.service))
    .send()
    .await?;
  resp.value().map_err(|_| page_error())
}

fn page_error() -> Error {
  Error::new(
    ErrorCode::VerificationRequired,
    "the passport answered with a page instead of data (risk control)",
  )
  .with_hint("log in with `--browser` from a browser where you are logged in")
}

/// The passport's own error, or `fallback`. A demanded identity check (scan
/// with the Douyin app) is `verification_required`.
fn failure(v: &Value, fallback: &str) -> Error {
  let message = v
    .first_str(&["data.description", "description", "message"])
    .unwrap_or_else(|| fallback.to_owned());
  if message.contains("验证") {
    return Error::new(ErrorCode::VerificationRequired, message)
      .with_hint("finish the identity check in the app, then log in again");
  }
  Error::upstream(format!("QR login: {message}"))
}

/// The passport's `mix_mode` encoding: every UTF-8 byte XOR 5, in hex
/// digits without padding.
pub(crate) fn mix(text: &str) -> String {
  text.bytes().map(|b| format!("{:x}", b ^ 5)).collect()
}

/// The `gfkadpd` cookie a glue page sets before the passport answers
/// (`var e="…",t="…"`), when `page` is one.
pub(crate) fn glue(page: &str) -> Option<String> {
  if !page.contains("gfkadpd") {
    return None;
  }
  let value = |name: &str| {
    let at = page.find(&format!("{name}=\""))? + name.len() + 2;
    let rest = &page[at..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (!digits.is_empty() && rest[digits.len()..].starts_with('"')).then_some(digits)
  };
  Some(format!("{},{}", value("var e")?, value("t")?))
}

/// A phone number as the passport takes it: `+86 13800000000`. Numbers
/// without a country code are taken as mainland China's.
pub fn mobile(phone: &str) -> Result<String> {
  let digits: String = phone
    .chars()
    .filter(|c| !c.is_whitespace() && *c != '-')
    .collect();
  let (code, number) = match digits.strip_prefix('+') {
    Some(rest) if rest.starts_with("86") => ("86", &rest[2..]),
    Some(rest) => match phone.trim().trim_start_matches('+').split_once(' ') {
      Some((code, _)) => (code, &rest[code.len()..]),
      None => return Err(Error::input("give the country code apart: `+852 51234567`")),
    },
    None => ("86", digits.as_str()),
  };
  let valid = !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit());
  if !valid || (code == "86" && number.len() != 11) {
    return Err(Error::input(format!("not a phone number: {phone}")));
  }
  Ok(format!("+{code} {number}"))
}

/// Text a login code to `mobile` (as [`mobile`] formats it).
pub async fn send_code(http: &Http, sso: Sso, mobile: &str) -> Result<()> {
  open(http, sso).await?;
  let form = [
    ("mix_mode", "1".to_owned()),
    ("mobile", mix(mobile)),
    ("type", "3731".into()),
    ("is6Digits", "1".into()),
    ("fixed_mix_mode", "1".into()),
  ];
  sso_post(http, sso, "/send_activation_code/v2/", &form).await?;
  Ok(())
}

/// Log in with the code sent to `mobile`; the ticket-guard answer
/// (`bd-ticket-guard-server-data`) of the login, if any.
pub async fn quick_login(
  http: &Http,
  sso: Sso,
  mobile: &str,
  code: &str,
) -> Result<Option<String>> {
  let form = [
    ("mix_mode", "1".to_owned()),
    ("mobile", mix(mobile)),
    ("code", mix(code.trim())),
    ("service", sso.service.into()),
    ("fixed_mix_mode", "1".into()),
  ];
  let (v, resp) = sso_post(http, sso, "/quick_login/v2/", &form).await?;
  let mut server_data = resp.header(SERVER_DATA).map(str::to_owned);
  let redirect = v
    .first_str(&["data.redirect_url", "redirect_url"])
    .filter(|url| same_site(url, sso.service));
  if let Some(url) = redirect {
    let last = finish(http, &url).await?;
    server_data = server_data.or_else(|| last.header(SERVER_DATA).map(str::to_owned));
  }
  Ok(server_data)
}

/// Open the passport's page: it sets `passport_csrf_token`.
async fn open(http: &Http, sso: Sso) -> Result<()> {
  http
    .get(format!("{}/", sso.host))
    .header("accept", "text/html,application/xhtml+xml")
    .send()
    .await?;
  Ok(())
}

async fn sso_post(
  http: &Http,
  sso: Sso,
  path: &str,
  form: &[(&str, String)],
) -> Result<(Value, Resp)> {
  for _ in 0..2 {
    let csrf = http.cookie("passport_csrf_token").unwrap_or_default();
    let trace = http.cookie("biz_trace_id").unwrap_or_else(|| {
      let id = format!("{:08x}", rand::random::<u32>());
      http.set_cookie("biz_trace_id", &id);
      id
    });
    let info = json!({"webdriver": false, "plugins": 5, "innerWidth": 1536, "innerHeight": 764});
    let resp = http
      .post(format!("{}{path}", sso.host))
      .query("passport_jssdk_version", "3.0.29")
      .query("passport_jssdk_type", "normal")
      .query("aid", sso.aid)
      .query("language", "zh")
      .query("account_sdk_source", "sso")
      .query("account_sdk_source_info", mix(&info.to_string()))
      .query("biz_trace_id", &trace)
      .header("accept", "application/json, text/javascript")
      .header("x-tt-passport-csrf-token", csrf)
      .header("x-tt-passport-trace-id", trace)
      .header("origin", sso.host)
      .header("referer", format!("{}/", sso.host))
      .form(form.iter().map(|(k, v)| (*k, v.as_str())))
      .send()
      .await?;
    tracing::debug!("{path}: {}", atp_core::text::truncate(&resp.text(), 300));
    if let Ok(v) = resp.value() {
      sms_check(&v, sso)?;
      return Ok((v, resp));
    }
    let Some(cookie) = glue(&resp.text()) else {
      break;
    };
    http.set_cookie("gfkadpd", &cookie);
  }
  Err(page_error())
}

/// Phone login refusals; a captcha (slider, identity check) is
/// `verification_required` on the site's page.
fn sms_check(v: &Value, sso: Sso) -> Result<()> {
  let code = v
    .i64("data.error_code")
    .or_else(|| v.i64("error_code"))
    .unwrap_or(0);
  if code == 0 {
    return Ok(());
  }
  let message = v
    .first_str(&["data.description", "description", "message"])
    .unwrap_or_else(|| format!("error_code {code}"));
  if [1104, 1105, 1107, 2046].contains(&code)
    || message.contains("验证")
    || message.to_lowercase().contains("captcha")
  {
    return Err(
      Error::verification(format!("the passport wants a captcha: {message}"))
        .with_url(format!("{}/", sso.service))
        .with_hint("pass the check on the page, then run the login again"),
    );
  }
  Err(Error::auth(format!("phone login: {message}")))
}

fn same_site(url: &str, service: &str) -> bool {
  let host = |u: &str| {
    url::Url::parse(u)
      .ok()
      .and_then(|u| u.host_str().map(str::to_owned))
  };
  let (Some(a), Some(b)) = (host(url), host(service)) else {
    return false;
  };
  let site = |h: &str| h.rsplitn(3, '.').take(2).collect::<Vec<_>>().join(".");
  site(&a) == site(&b)
}
