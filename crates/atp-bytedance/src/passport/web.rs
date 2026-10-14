//! The passport of ByteDance's account web SDK (`passport_jssdk` 3.4.4,
//! `/passport/web/…`), as login.douyin.com serves www.douyin.com: QR logins
//! signed with the SDK's own `sign` / `qs` and `x-tt-passport-aid-sign`,
//! plus `a_bogus` over the query and the body. Parameters as
//! `cv-cat/catbus` (`src/platforms/douyin/web/passport.ts`, `sign.ts`, MIT)
//! records them; the page's script challenges (acrawler, the challenge
//! template) are not run.

use atp_core::http::{Http, Req, Resp, USER_AGENT};
use atp_core::text::truncate;
use atp_core::{Error, Result, Value, ValueExt, json};
use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::{Digest, Sha256};

use super::{Qr, QrState, glue, mix};
use crate::abogus::{self, App};
use crate::query::{encode, query_string};
use crate::ticket_guard::SERVER_DATA;

/// One site's web passport.
#[derive(Debug, Clone, Copy)]
pub struct Web {
  /// The passport host.
  pub host: &'static str,
  /// The site whose pages call it.
  pub origin: &'static str,
  pub aid: u64,
  /// The `a_bogus` ids the site signs passport calls with.
  pub app: App,
}

pub const DOUYIN: Web = Web {
  host: "https://login.douyin.com",
  origin: "https://www.douyin.com",
  aid: 6383,
  app: abogus::DOUYIN,
};

const APP_KEY: &str = "163e7ce78d58971a41f5b969996d85c2";
const GET_QRCODE: &str = "/passport/web/get_qrcode/";
const CHECK_QRCONNECT: &str = "/passport/web/check_qrconnect/";
const SDK: [(&str, &str); 3] = [
  ("passport_jssdk_version", "3.4.4"),
  ("passport_jssdk_type", "normal"),
  ("is_from_ttaccountsdk", "1"),
];
const SDK_TAIL: [(&str, &str); 5] = [
  ("p_js_v", "3.4.4"),
  ("p_js_t", "pro"),
  ("p_zt", "3.3.17"),
  ("p_ver", "1.1.3"),
  ("p_ver_real", "0"),
];
/// Parameters the SDK leaves out of `sign`.
const UNSIGNED: [&str; 4] = ["sign", "qs", "msToken", "a_bogus"];

pub async fn qr_start(http: &Http, web: Web) -> Result<Qr> {
  let extra = [
    ("next", web.origin),
    ("need_short_url", "true"),
    ("need_logo", "false"),
    ("is_new_login", "1"),
    ("is_from_iesaccountsaas", "1"),
  ];
  let (v, _) = call(http, web, GET_QRCODE, &extra, &[], false).await?;
  let data = v.at("data");
  match (data.str("qrcode_index_url"), data.str("token")) {
    (Some(url), Some(token)) => Ok(Qr { url, token }),
    _ => Err(super::failure(&v, "the passport issued no QR code")),
  }
}

/// The state of a QR login and the ticket-guard answer it carried
/// (`bd-ticket-guard-server-data`), if any.
pub async fn qr_poll(http: &Http, web: Web, token: &str) -> Result<(QrState, Option<String>)> {
  let form = [
    ("need_logo", "false"),
    ("is_frontier", "true"),
    ("token", token),
    ("is_new_login", "1"),
    ("next", web.origin),
    ("need_short_url", "true"),
  ];
  let extra = [("is_from_iesaccountsaas", "1")];
  let (v, resp) = call(http, web, CHECK_QRCONNECT, &extra, &form, true).await?;
  let server_data = resp.header(SERVER_DATA).map(str::to_owned);
  Ok((super::qr_state(&v)?, server_data))
}

/// One passport call; a POST when `form` is not empty. A first answer
/// that is the `gfkadpd` glue page sets its cookie and is asked again.
async fn call(
  http: &Http,
  web: Web,
  path: &str,
  extra: &[(&str, &str)],
  form: &[(&str, &str)],
  device: bool,
) -> Result<(Value, Resp)> {
  let form: Vec<(String, String)> = form
    .iter()
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect();
  let body = query_string(&form);
  for _ in 0..2 {
    let query = signed(http, web, extra, &form, &body, device);
    let url = format!("{}{path}?{query}", web.host);
    let req = if form.is_empty() {
      http.get(url)
    } else {
      http.post(url).bytes(
        body.clone().into_bytes(),
        "application/x-www-form-urlencoded",
      )
    };
    let resp = headers(http, web, path, req).send().await?;
    tracing::debug!("{path}: {}", truncate(&resp.text(), 300));
    if let Ok(v) = resp.value() {
      check(&v)?;
      return Ok((v, resp));
    }
    let Some(cookie) = glue(&resp.text()) else {
      break;
    };
    http.set_cookie("gfkadpd", &cookie);
  }
  Err(
    Error::verification("the passport answered with a page instead of data (risk control)")
      .with_hint("log in with `--browser` or `--cookie` instead")
      .unclearable(),
  )
}

/// Refusals: 4031 is the passport blocking this client outright.
fn check(v: &Value) -> Result<()> {
  match v.i64("data.error_code").unwrap_or(0) {
    0 | 7 => Ok(()),
    4031 => Err(
      Error::verification(format!(
        "the passport refused this client: {}",
        v.str("data.description").unwrap_or_default()
      ))
      .with_hint("log in with `--browser` or `--cookie` instead")
      .unclearable(),
    ),
    _ => Err(super::failure(v, "the passport refused the request")),
  }
}

fn headers<'a>(http: &Http, web: Web, path: &str, req: Req<'a>) -> Req<'a> {
  let csrf = http
    .cookie("passport_csrf_token")
    .or_else(|| http.cookie("passport_csrf_token_default"))
    .unwrap_or_default();
  req
    .header("web-sdk-version", "1")
    .header("x-tt-passport-aid-sign", aid_sign(web.aid, path, &sdk_ts()))
    .header("x-tt-passport-csrf-token", csrf)
    .header("x-tt-passport-trace-id", trace_id(http))
    .header("accept", "application/json, text/javascript")
    .header("origin", web.origin)
    .header("referer", format!("{}/", web.origin))
    .header("sec-fetch-site", "same-site")
}

/// The query as sent: the SDK's parameters, `sign`, `qs`, `msToken`, then
/// `a_bogus` over everything before it and the body.
fn signed(
  http: &Http,
  web: Web,
  extra: &[(&str, &str)],
  form: &[(String, String)],
  body: &str,
  device: bool,
) -> String {
  let mut pairs = sdk_pairs(http, web, extra, device);
  let (sign, qs) = sign(&pairs, form);
  pairs.push(("sign".into(), sign));
  pairs.push(("qs".into(), qs));
  pairs.push(("msToken".into(), http.cookie("msToken").unwrap_or_default()));
  let query = query_string(&pairs);
  let now = jiff::Timestamp::now().as_millisecond();
  let a_bogus = abogus::sign(web.app, &query, body, USER_AGENT, now);
  format!("{query}&a_bogus={}", encode(&a_bogus))
}

fn sdk_pairs(http: &Http, web: Web, extra: &[(&str, &str)], device: bool) -> Vec<(String, String)> {
  let mut pairs: Vec<(&str, String)> = SDK.iter().map(|(k, v)| (*k, (*v).to_owned())).collect();
  pairs.push(("aid", web.aid.to_string()));
  pairs.push(("language", "zh".into()));
  pairs.push(("account_app_language", "zh-CN".into()));
  pairs.push(("ts", sdk_ts()));
  pairs.extend(extra.iter().map(|(k, v)| (*k, (*v).to_owned())));
  pairs.push(("p_ui", "2.4.4".into()));
  if device {
    let fp = http.cookie("s_v_web_id").unwrap_or_default();
    pairs.push(("p_ca", "4.0.26".into()));
    pairs.push(("p_ca_real", "1.0.0.892".into()));
    pairs.push(("fp", fp.clone()));
    pairs.push(("verifyFp", fp));
  }
  pairs.push(("account_sdk_source", "web".into()));
  pairs.push(("account_sdk_source_info", source_info(web.origin)));
  pairs.extend(SDK_TAIL.iter().map(|(k, v)| (*k, (*v).to_owned())));
  // The SDK encodes this value once itself.
  pairs.push(("request_host", encode(web.origin)));
  pairs.push(("p_bd", "1.0.1.19-fix.01".into()));
  pairs.push(("p_ts", jiff::Timestamp::now().as_millisecond().to_string()));
  pairs.push(("p_no", random_hex(32)));
  pairs.push(("biz_trace_id", trace_id(http)));
  pairs.push(("device_platform", "web_app".into()));
  pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

/// `sign`: SHA-256 over the first ten parameters by name, the body and
/// the SDK's app key; `qs` names those parameters.
fn sign(query: &[(String, String)], form: &[(String, String)]) -> (String, String) {
  let mut keys: Vec<&str> = query
    .iter()
    .map(|(k, _)| k.as_str())
    .filter(|k| !UNSIGNED.contains(k))
    .collect();
  keys.sort_unstable();
  keys.dedup();
  keys.truncate(10);
  let value = |pairs: &[(String, String)], key: &str| {
    pairs
      .iter()
      .rev()
      .find(|(k, _)| k == key)
      .map(|(_, v)| v.clone())
      .unwrap_or_default()
  };
  let qstr: Vec<String> = keys
    .iter()
    .map(|k| format!("{k}={}", value(query, k)))
    .collect();
  let mut fields: Vec<&str> = form.iter().map(|(k, _)| k.as_str()).collect();
  fields.sort_unstable();
  let bstr: Vec<String> = fields
    .iter()
    .map(|k| format!("{k}={}", value(form, k)))
    .collect();
  let text = format!("{}&{}&app_key={APP_KEY}", qstr.join("&"), bstr.join("&"));
  (hex(&Sha256::digest(text.as_bytes())), mix(&keys.join(",")))
}

/// `x-tt-passport-aid-sign`: HMAC-SHA256 of aid, path and `ts` under a
/// key derived (HKDF-style) from the app key.
fn aid_sign(aid: u64, path: &str, ts: &str) -> String {
  let prk = hmac(ts.as_bytes(), APP_KEY.as_bytes());
  let okm = hmac(&prk, &[1]);
  hex(&hmac(
    &okm,
    format!("aid={aid}&path={path}&ts={ts}").as_bytes(),
  ))
}

fn hmac(key: &[u8], message: &[u8]) -> Vec<u8> {
  let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("any key length");
  mac.update(message);
  mac.finalize().into_bytes().to_vec()
}

/// The SDK's `ts`: noon (UTC) of the current day, in seconds.
fn sdk_ts() -> String {
  let now = jiff::Timestamp::now().as_second();
  (now - now.rem_euclid(86_400) + 43_200).to_string()
}

/// `biz_trace_id`: one per session, kept as the cookie the page sets.
fn trace_id(http: &Http) -> String {
  if let Some(id) = http.cookie("biz_trace_id") {
    return id;
  }
  let id = random_hex(4);
  http.set_cookie("biz_trace_id", &id);
  id
}

/// `account_sdk_source_info`: what the page reports about the browser.
fn source_info(origin: &str) -> String {
  let mut rng = rand::rng();
  let now = jiff::Timestamp::now().as_millisecond();
  let started = now - rng.random_range(3_000..60_000);
  let info = json!({
    "hardwareConcurrency": 8,
    "webdriver": false,
    "chromedriver": false,
    "shelldriver": false,
    "plugins": 5,
    "innerHeight": 764,
    "innerWidth": 1536,
    "outerHeight": 864,
    "outerWidth": 1536,
    "automation": {"s": "00000000", "c": "0000", "p": "0000000", "s1": "00000000", "c1": "0000", "p1": "0"},
    "performance": {
      "timeOrigin": started as f64 + 0.5,
      "navigationTiming": {
        "entryType": "navigation",
        "initiatorType": "navigation",
        "name": format!("{origin}/?recommend=1"),
        "renderBlockingStatus": "non-blocking",
      },
    },
    "browser": {"t": rng.random_range(1_000_000_000_000u64..10_000_000_000_000), "bit_protocol": "false", "bit_helper": false},
  });
  mix(&info.to_string())
}

fn random_hex(bytes: usize) -> String {
  let mut rng = rand::rng();
  (0..bytes)
    .map(|_| format!("{:02x}", rng.random::<u8>()))
    .collect()
}

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}
