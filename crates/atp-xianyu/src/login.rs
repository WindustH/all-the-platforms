//! QR login through Alibaba's Havana passport for Xianyu
//! (`passport.goofish.com`, atp-alibaba's [`havana`]): the mini login page
//! hands out a CSRF token, `generate.do` the code, `query.do` its state. A
//! confirmed scan brings a login token that `login_token/login.do` trades
//! for the session cookies. When the passport wants a face or SMS check
//! first, that is `verification_required` with the page to do it on.

use atp_alibaba::{havana, mtop};
use atp_core::http::USER_AGENT;
use atp_core::{QrStatus, QrTicket, Result, Value, ValueExt};

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
  let code = havana::code(&v, "Xianyu")?;
  Ok(QrTicket {
    url: code.content,
    token: code.t,
    extra: [("ck".to_owned(), code.ck), ("csrf".to_owned(), csrf)].into(),
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
  let (status, data) = havana::status(&v)?;
  if status == QrStatus::Confirmed {
    confirm(api, data).await?;
  }
  Ok(status)
}

/// Trade the confirmed scan for the session cookies.
async fn confirm(api: &Api, data: &Value) -> Result<()> {
  let http = &api.ctx.http;
  let mut redirect = havana::redirect(data);
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
      redirect = redirect.or_else(|| havana::redirect(v.at("content.data")));
    }
  }
  if !http.has_cookie("unb") {
    return Err(havana::check_wanted(
      "Xianyu",
      "xianyu",
      "www.goofish.com",
      redirect,
    ));
  }
  // The login changes the signing token; fetch the new one.
  mtop::touch(&api.ctx, &mtop::GOOFISH, "mtop.idle.web.user.page.nav").await
}
