//! QR login to the mp.weixin.qq.com backend, as its login page does it:
//! `bizlogin?action=startlogin` opens a QR session (cookie `uuid`), the code
//! comes as an image (decoded and redrawn by the core), `scanloginqrcode
//! ?action=ask` reports progress, and `bizlogin?action=login` turns the
//! confirmed scan into session cookies and a token.

use social_core::{Error, QrStatus, QrTicket, Result, ValueExt};

use crate::api::Api;
use crate::backend::MP;

fn session_id() -> String {
  let ms = jiff::Timestamp::now().as_millisecond();
  format!("{ms}{:02}", rand::random::<u8>() % 100)
}

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  // Leftovers of an older session would confuse the login page.
  api.ctx.http.replace_cookies(Default::default());
  let _ = api.ctx.http.get(format!("{MP}/")).send().await;
  let sid = session_id();
  let form = [
    ("userlang", "zh_CN".to_owned()),
    ("redirect_url", String::new()),
    ("login_type", "3".into()),
    ("sessionid", sid.clone()),
    ("token", String::new()),
    ("lang", "zh_CN".into()),
    ("f", "json".into()),
    ("ajax", "1".into()),
    ("fingerprint", api.backend().fingerprint()),
  ];
  let v = api
    .ctx
    .http
    .post(format!("{MP}/cgi-bin/bizlogin"))
    .query("action", "startlogin")
    .header("origin", MP)
    .header("referer", format!("{MP}/"))
    .header("x-requested-with", "XMLHttpRequest")
    .form(form)
    .value()
    .await?;
  if v.i64("base_resp.ret").unwrap_or(0) != 0 {
    return Err(Error::upstream(format!(
      "could not start the QR login: {v}"
    )));
  }
  let resp = api
    .ctx
    .http
    .get(format!("{MP}/cgi-bin/scanloginqrcode"))
    .query("action", "getqrcode")
    .query("random", jiff::Timestamp::now().as_millisecond())
    .header("referer", format!("{MP}/"))
    .header(
      "accept",
      "image/avif,image/webp,image/png,image/*,*/*;q=0.8",
    )
    .send()
    .await?
    .check()?;
  if social_core::file::sniff(&resp.body).is_none() {
    return Err(Error::upstream(
      "the login QR code did not come as an image",
    ));
  }
  Ok(QrTicket {
    token: sid,
    image: Some(resp.body),
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, _ticket: &QrTicket) -> Result<QrStatus> {
  let v = api
    .ctx
    .http
    .get(format!("{MP}/cgi-bin/scanloginqrcode"))
    .queries([
      ("action", "ask".to_owned()),
      ("token", String::new()),
      ("lang", "zh_CN".into()),
      ("f", "json".into()),
      ("ajax", "1".into()),
      ("fingerprint", api.backend().fingerprint()),
    ])
    .header("referer", format!("{MP}/"))
    .header("x-requested-with", "XMLHttpRequest")
    .value()
    .await?;
  Ok(match v.i64("status").unwrap_or(0) {
    0 => QrStatus::Waiting,
    4 | 6 if v.i64("acct_size") == Some(0) => {
      return Err(Error::auth(
        "this WeChat user administers no official account it can log in to",
      ));
    }
    4 | 6 => QrStatus::Scanned,
    1 => {
      finish(api).await?;
      QrStatus::Confirmed
    }
    5 => {
      return Err(
        Error::auth("WeChat refuses the QR login: the account has no email bound")
          .with_hint("bind an email in the mp.weixin.qq.com settings, then log in again"),
      );
    }
    _ => QrStatus::Expired,
  })
}

/// Trade the confirmed scan for the session cookies and the token.
async fn finish(api: &Api) -> Result<()> {
  let form = [
    ("userlang", "zh_CN".to_owned()),
    ("redirect_url", String::new()),
    ("cookie_forbidden", "0".into()),
    ("cookie_cleaned", "0".into()),
    ("plugin_used", "0".into()),
    ("login_type", "3".into()),
    ("fingerprint", api.backend().fingerprint()),
    ("token", String::new()),
    ("lang", "zh_CN".into()),
    ("f", "json".into()),
    ("ajax", "1".into()),
  ];
  let v = api
    .ctx
    .http
    .post(format!("{MP}/cgi-bin/bizlogin"))
    .query("action", "login")
    .header("origin", MP)
    .header("referer", format!("{MP}/"))
    .header("x-requested-with", "XMLHttpRequest")
    .form(form)
    .value()
    .await?;
  let v = crate::backend::check(v)?;
  let redirect = v.str("redirect_url").unwrap_or_default();
  let token = redirect
    .split(['?', '&'])
    .find_map(|p| p.strip_prefix("token="))
    .filter(|t| !t.is_empty())
    .ok_or_else(|| Error::upstream(format!("the login gave no token: {redirect}")))?;
  api.backend().set_token(token);
  // Land on the home page like the browser does.
  let _ = api
    .ctx
    .http
    .get(format!("{MP}{redirect}"))
    .header("referer", format!("{MP}/"))
    .send()
    .await;
  Ok(())
}
