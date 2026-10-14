//! QR login through Alibaba's passport for Xianyu (`passport.goofish.com`):
//! the mini login page hands out a CSRF token, `generate.do` the code,
//! `query.do` its state. A confirmed scan brings a login token that
//! `login_token/login.do` trades for the session cookies. When the passport
//! wants a face or SMS check first, that is `verification_required` with
//! the page to do it on.

use atp_alibaba::mtop;
use atp_core::http::USER_AGENT;
use atp_core::{Error, ErrorCode, QrStatus, QrTicket, Result, Value, ValueExt};

use crate::api::{Api, WWW};

const PASSPORT: &str = "https://passport.goofish.com";
const MINI_LOGIN: &str = "https://passport.goofish.com/mini_login.htm";

/// What every passport call carries.
fn login_params(csrf: &str, cookie2: &str) -> Vec<(&'static str, String)> {
  vec![
    ("appName", "xianyu".into()),
    ("fromSite", "77".into()),
    ("appEntrance", "web".into()),
    ("_csrf_token", csrf.into()),
    ("umidToken", String::new()),
    ("hsiz", cookie2.into()),
    (
      "bizParams",
      "taobaoBizLoginFrom=web&renderRefer=https%3A//www.goofish.com/".into(),
    ),
    ("mainPage", "false".into()),
    ("isMobile", "false".into()),
    ("lang", "zh_CN".into()),
    ("returnUrl", String::new()),
    ("umidTag", "SERVER".into()),
  ]
}

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  api.guest().await;
  let http = &api.ctx.http;
  if !http.has_cookie("cookie2") {
    mtop::touch(
      &api.ctx,
      &mtop::GOOFISH,
      "mtop.gaia.nodejs.gaia.idle.data.gw.v2.index.get",
    )
    .await?;
  }
  http
    .get(MINI_LOGIN)
    .queries([
      ("lang", "zh_cn"),
      ("appName", "xianyu"),
      ("appEntrance", "web"),
      ("styleType", "vertical"),
      ("bizParams", ""),
      ("notLoadSsoView", "false"),
      ("notKeepLogin", "false"),
      ("isMobile", "false"),
      ("qrCodeFirst", "false"),
      ("stie", "77"),
    ])
    .header("referer", format!("{WWW}/"))
    .header("accept", "text/html,application/xhtml+xml")
    .send()
    .await?;
  let csrf = http.cookie("XSRF-TOKEN").unwrap_or_default();
  let cookie2 = http.cookie("cookie2").unwrap_or_default();
  let v = http
    .get(format!("{PASSPORT}/newlogin/qrcode/generate.do"))
    .queries(login_params(&csrf, &cookie2))
    .header("referer", MINI_LOGIN)
    .send()
    .await?
    .value()?;
  let data = v.at("content.data");
  let (Some(code), Some(t)) = (data.str("codeContent"), data.str("t")) else {
    return Err(Error::upstream("the Xianyu passport issued no QR code"));
  };
  Ok(QrTicket {
    url: code,
    token: t,
    extra: [
      ("ck".to_owned(), data.str("ck").unwrap_or_default()),
      ("csrf".to_owned(), csrf),
    ]
    .into(),
    image: None,
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let http = &api.ctx.http;
  let extra = |k: &str| ticket.extra.get(k).cloned().unwrap_or_default();
  let cookie2 = http.cookie("cookie2").unwrap_or_default();
  let mut form = login_params(&extra("csrf"), &cookie2);
  form.extend([
    ("navlanguage", "zh-CN".into()),
    ("navUserAgent", USER_AGENT.into()),
    ("navPlatform", "MacIntel".into()),
    ("isIframe", "true".into()),
    ("documentReferer", format!("{WWW}/")),
    ("defaultView", "sms".into()),
    ("deviceId", http.cookie("cna").unwrap_or_default()),
    ("t", ticket.token.clone()),
    ("ck", extra("ck")),
  ]);
  let v = http
    .post(format!(
      "{PASSPORT}/newlogin/qrcode/query.do?appName=xianyu&fromSite=77"
    ))
    .form(form)
    .header("origin", PASSPORT)
    .header("referer", MINI_LOGIN)
    .send()
    .await?
    .value()?;
  let data = v.at("content.data");
  Ok(match data.str("qrCodeStatus").as_deref() {
    Some("SCANED" | "SCANNED") => QrStatus::Scanned,
    Some("CONFIRMED") => {
      confirm(api, data).await?;
      QrStatus::Confirmed
    }
    Some("EXPIRED" | "CANCELED" | "CANCELLED") => QrStatus::Expired,
    _ => QrStatus::Waiting,
  })
}

/// Trade the confirmed scan for the session cookies.
async fn confirm(api: &Api, data: &Value) -> Result<()> {
  let http = &api.ctx.http;
  let mut redirect = redirect_of(data);
  if let Some(token) = data.first_str(&["token", "lgToken"]) {
    let answer = http
      .post(format!("{PASSPORT}/login_token/login.do"))
      .queries([
        ("token", token.as_str()),
        ("subFlow", "DIALOG_CHECK_LOGIN_RPC"),
        ("nextCode", "0018"),
        ("bizScene", "qrcode"),
        ("confirm", "true"),
      ])
      .form([("deviceId", http.cookie("cna").unwrap_or_default())])
      .header("origin", PASSPORT)
      .header("referer", MINI_LOGIN)
      .send()
      .await?;
    if let Ok(v) = answer.value() {
      redirect = redirect.or_else(|| redirect_of(v.at("content.data")));
    }
  }
  if !http.has_cookie("unb") {
    let hint = match redirect {
      Some(url) => {
        format!("open {url} in a browser to finish, then run `atp xianyu login --browser`")
      }
      None => {
        "log in on www.goofish.com in a browser, then run `atp xianyu login --browser`".into()
      }
    };
    return Err(
      Error::new(
        ErrorCode::VerificationRequired,
        "Xianyu wants to confirm this login (a face or SMS check)",
      )
      .with_hint(hint),
    );
  }
  // The login changes the signing token; fetch the new one.
  mtop::touch(&api.ctx, &mtop::GOOFISH, "mtop.idle.web.user.page.nav").await
}

fn redirect_of(data: &Value) -> Option<String> {
  data.first_str(&["iframeRedirectUrl", "redirectUrl"])
}
