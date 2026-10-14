//! QR login through QQ, as y.qq.com logs in:
//!
//! 1. `ptqrshow` hands out the QR image and the cookie `qrsig`;
//! 2. `ptqrlogin`, polled with `ptqrtoken` ([`sign::hash33`] of `qrsig`),
//!    answers `ptuiCB('66', …)` until the QQ app scans (67) and confirms
//!    (0, with the `check_sig` link);
//! 3. `check_sig` sets the QQ cookies, among them `p_skey`;
//! 4. `graph.qq.com/oauth2.0/authorize` (with the `g_tk` of `p_skey`)
//!    redirects to y.qq.com with a `code`;
//! 5. `QQConnectLogin.LoginServer.QQLogin` trades the code for the music
//!    key and the account number, which become the session; the QQ
//!    cookies are dropped.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use atp_core::{Error, ErrorCode, QrStatus, QrTicket, Result, Value, ValueExt};
use regex::Regex;

use crate::api::{Api, SESSION_COOKIES};
use crate::sign;
use crate::users;

const PTLOGIN: &str = "https://ssl.ptlogin2.qq.com";
const PTLOGIN_REFERER: &str = "https://xui.ptlogin2.qq.com/";
/// QQ Music's applet on QQ login and its QQ Connect app.
const APP_ID: &str = "716027609";
const DOMAIN_ID: &str = "383";
const CONNECT_APP: &str = "100497308";
const AUTHORIZE: &str = "https://graph.qq.com/oauth2.0/authorize";
const RETURN_TO: &str =
  "https://y.qq.com/portal/wx_redirect.html?login_type=1&surl=https://y.qq.com/";
/// `tmeLoginType` of a QQ login.
const QQ_LOGIN: u64 = 2;

static ARGS: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"'((?:\\.|[^'])*)'").expect("valid regex"));
static CODE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"[?&]code=([^&#]+)").expect("valid regex"));

fn now_ms() -> i64 {
  jiff::Timestamp::now().as_millisecond()
}

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let http = &api.ctx.http;
  // A fresh jar: nothing of an older session goes to QQ login.
  http.replace_cookies(BTreeMap::new());
  let resp = http
    .get(format!("{PTLOGIN}/ptqrshow"))
    .queries([
      ("appid", APP_ID.to_owned()),
      ("e", "2".to_owned()),
      ("l", "M".to_owned()),
      ("s", "3".to_owned()),
      ("d", "72".to_owned()),
      ("v", "4".to_owned()),
      (
        "t",
        format!("0.{}", rand::random_range(10_000_000..99_999_999u64)),
      ),
      ("daid", DOMAIN_ID.to_owned()),
      ("pt_3rd_aid", CONNECT_APP.to_owned()),
    ])
    .header("referer", PTLOGIN_REFERER)
    .send()
    .await?
    .check()?;
  let qrsig = http
    .cookie("qrsig")
    .ok_or_else(|| Error::upstream("QQ login sent no qrsig with its QR code"))?;
  Ok(QrTicket {
    url: resp.url,
    token: qrsig,
    image: Some(resp.body),
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let text = api
    .ctx
    .http
    .get(format!("{PTLOGIN}/ptqrlogin"))
    .queries([
      ("u1", "https://graph.qq.com/oauth2.0/login_jump".to_owned()),
      ("ptqrtoken", sign::hash33(&ticket.token, 0).to_string()),
      ("ptredirect", "0".to_owned()),
      ("h", "1".to_owned()),
      ("t", "1".to_owned()),
      ("g", "1".to_owned()),
      ("from_ui", "1".to_owned()),
      ("ptlang", "2052".to_owned()),
      ("action", format!("0-0-{}", now_ms())),
      ("js_ver", "20102616".to_owned()),
      ("js_type", "1".to_owned()),
      ("pt_uistyle", "40".to_owned()),
      ("aid", APP_ID.to_owned()),
      ("daid", DOMAIN_ID.to_owned()),
      ("pt_3rd_aid", CONNECT_APP.to_owned()),
      ("has_onekey", "1".to_owned()),
    ])
    .header("referer", PTLOGIN_REFERER)
    .send()
    .await?
    .check()?
    .text();
  let args: Vec<String> = ARGS
    .captures_iter(&text)
    .map(|c| c[1].replace("\\'", "'"))
    .collect();
  match args.first().map(String::as_str) {
    Some("66") => Ok(QrStatus::Waiting),
    Some("67") => Ok(QrStatus::Scanned),
    Some("65") => Ok(QrStatus::Expired),
    Some("68") => Err(Error::auth("the login was refused on the phone")),
    Some("0") => {
      let link = args.get(2).cloned().unwrap_or_default();
      finish(api, &link).await?;
      Ok(QrStatus::Confirmed)
    }
    _ => Err(refused(&args)),
  }
}

/// What QQ login answers when it wants more than a scan: a page to pass
/// when it names one.
fn refused(args: &[String]) -> Error {
  let message = args
    .get(4)
    .filter(|m| !m.is_empty())
    .cloned()
    .unwrap_or_else(|| {
      format!(
        "QQ login answered {}",
        args.first().map_or("nothing", |a| a)
      )
    });
  let error = Error::new(
    ErrorCode::VerificationRequired,
    format!("QQ login: {message}"),
  )
  .with_hint("QQ wants a check: open the page (or log in at https://y.qq.com and use --browser)");
  match args.get(2).filter(|u| u.starts_with("http")) {
    Some(url) => error.with_url(url.clone()),
    None => error,
  }
}

/// From the confirmed scan to a QQ Music session.
async fn finish(api: &Api, check_sig: &str) -> Result<()> {
  let http = &api.ctx.http;
  if !check_sig.starts_with("https://") {
    return Err(Error::upstream(
      "QQ login confirmed without a check_sig link",
    ));
  }
  http
    .get(check_sig)
    .header("referer", PTLOGIN_REFERER)
    .send()
    .await?;
  let p_skey = http
    .cookie("p_skey")
    .ok_or_else(|| Error::upstream("QQ login: check_sig set no p_skey"))?;
  let code = authorize(api, &p_skey).await?;
  let data = api
    .call("QQConnectLogin.LoginServer", "QQLogin")
    .arg("code", code)
    .send_as(QQ_LOGIN)
    .await?;
  keep_session(api, &data)
}

/// The QQ Connect code for QQ Music, from where the authorization page
/// redirects.
async fn authorize(api: &Api, p_skey: &str) -> Result<String> {
  let ui = format!("{:032x}", rand::random::<u128>());
  let resp = api
    .ctx
    .http
    .post(AUTHORIZE)
    .header("referer", "https://graph.qq.com/")
    .form([
      ("response_type", "code".to_owned()),
      ("client_id", CONNECT_APP.to_owned()),
      ("redirect_uri", RETURN_TO.to_owned()),
      ("scope", "get_user_info,get_app_friends".to_owned()),
      ("state", "state".to_owned()),
      ("switch", String::new()),
      ("from_ptlogin", "1".to_owned()),
      ("src", "1".to_owned()),
      ("update_auth", "1".to_owned()),
      ("openapi", "1010_1030".to_owned()),
      ("g_tk", sign::hash33(p_skey, 5381).to_string()),
      ("auth_time", now_ms().to_string()),
      ("ui", ui),
    ])
    .send()
    .await?;
  CODE
    .captures(&resp.url)
    .map(|c| c[1].to_owned())
    .ok_or_else(|| {
      Error::upstream(format!(
        "QQ Connect gave no code for QQ Music (ended at {})",
        resp.url.split('?').next().unwrap_or_default()
      ))
    })
}

/// The music cookies of a `QQLogin` answer become the whole session.
fn keep_session(api: &Api, data: &Value) -> Result<()> {
  let http = &api.ctx.http;
  let uin = data
    .first_str(&["musicid", "str_musicid"])
    .filter(|u| u != "0");
  let key = data.first_str(&["musickey", "music_key"]);
  let (Some(uin), Some(key)) = (uin, key) else {
    return Err(Error::upstream("QQLogin answered without a music key"));
  };
  for (cookie, path) in [
    ("psrf_qqopenid", "openid"),
    ("psrf_qqunionid", "unionid"),
    ("psrf_qqaccess_token", "access_token"),
    ("psrf_qqrefresh_token", "refresh_token"),
    ("psrf_access_token_expiresAt", "expired_at"),
    ("psrf_musickey_createtime", "musickeyCreateTime"),
  ] {
    if let Some(value) = data.str(path) {
      http.set_cookie(cookie, &value);
    }
  }
  http.set_cookie("uin", &uin);
  http.set_cookie("qqmusic_key", &key);
  http.set_cookie("qm_keyst", &key);
  http.set_cookie("tmeLoginType", &QQ_LOGIN.to_string());
  keep_music_cookies(api);
  if let Some(euin) = data.first_str(&["encryptUin", "encrypt_uin"]) {
    api.ctx.set_extra(users::EUIN, &euin);
  }
  Ok(())
}

/// Drop every cookie a QQ Music session does not consist of.
pub fn keep_music_cookies(api: &Api) {
  let http = &api.ctx.http;
  let jar: BTreeMap<String, String> = http
    .cookies()
    .into_iter()
    .filter(|(name, _)| SESSION_COOKIES.contains(&name.as_str()))
    .collect();
  http.replace_cookies(jar);
}

/// After `--cookie` or `--browser`: only the music cookies, and the key
/// under both of its names.
pub fn prepare(api: &Api) {
  let http = &api.ctx.http;
  if let Some(key) = api.key() {
    for name in ["qqmusic_key", "qm_keyst"] {
      if !http.has_cookie(name) {
        http.set_cookie(name, &key);
      }
    }
  }
  keep_music_cookies(api);
}
