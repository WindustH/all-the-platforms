//! Login: the QR code for the Zhihu app.

use social_core::{Error, ErrorCode, QrStatus, QrTicket, Result, Value, ValueExt};

use crate::api::{Api, V3, V4, WWW};

const QR_API: &str = "https://www.zhihu.com/api/v3/account/api/login/qrcode";

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  // Start from a clean jar: an expired `z_c0` would confuse the flow.
  api.ctx.http.replace_cookies(Default::default());
  let _ = api
    .ctx
    .http
    .get(format!("{WWW}/signin"))
    .header("accept", "text/html,application/xhtml+xml")
    .send()
    .await?;
  api.helper_cookies().await?;
  // Issues the `capsion_ticket` cookie the confirmation step checks.
  let _ = api
    .get(&format!("{V3}/oauth/captcha/v2"))
    .query("type", "captcha_sign_in")
    .send()
    .await?;
  let v = api
    .call_public(
      api
        .post(QR_API)
        .header("referer", format!("{WWW}/signin"))
        .header("origin", WWW)
        .json(&serde_json::json!({})),
    )
    .await?;
  let token = v
    .first_str(&["token", "qrcode_token"])
    .ok_or_else(|| Error::upstream("the QR code API returned no token"))?;
  let url = v
    .str("link")
    .ok_or_else(|| Error::upstream("the QR code API returned no link"))?;
  let ticket = QrTicket {
    url,
    token,
    ..QrTicket::default()
  };
  // Poll once before the code is shown, so a blocked network fails before anyone scans.
  qr_poll(api, &ticket).await?;
  Ok(ticket)
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let resp = api
    .get(&format!("{QR_API}/{}/scan_info", ticket.token))
    .header("referer", format!("{WWW}/signin?next=%2F"))
    .header("accept", "*/*")
    .header("sec-fetch-dest", "empty")
    .header("sec-fetch-mode", "cors")
    .header("sec-fetch-site", "same-origin")
    .header("x-zse-93", "101_3_3.0")
    .send()
    .await?;
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or(Value::Null);
  let message = v.str("error.message").unwrap_or_default();
  if ["过期", "失效", "expire"]
    .iter()
    .any(|w| message.contains(w))
  {
    return Ok(QrStatus::Expired);
  }
  if !resp.status.is_success() {
    let err = api.error(resp.status.as_u16(), &v, &resp.text());
    // Risk control does not clear by polling: report it instead of waiting out the timeout.
    if err.code == ErrorCode::VerificationRequired {
      let check = v
        .str("error.redirect")
        .map(|url| format!("pass the check in a browser ({url}), "))
        .unwrap_or_default();
      return Err(err.with_hint(format!(
        "Zhihu flagged this network; {check}retry later or through another network (`--proxy`), or log in with `--browser` / `--cookie`"
      )));
    }
    tracing::debug!("scan_info {}: {}", resp.status, resp.text());
    // Other failures may be transient: keep polling until the core gives up.
    return Ok(QrStatus::Waiting);
  }
  absorb_cookies(api, &v);
  let confirmed = v.str("access_token").is_some() || v.str("user_id").is_some();
  match v.i64("status") {
    _ if api.ctx.http.has_cookie("z_c0") => Ok(QrStatus::Confirmed),
    Some(0) => Ok(QrStatus::Waiting),
    Some(1) => Ok(QrStatus::Scanned),
    _ if confirmed => {
      // The cookie usually arrives with the confirmation; ask once more if it did not.
      let _ = api.get(&format!("{V4}/me")).send().await?;
      if api.ctx.http.has_cookie("z_c0") {
        Ok(QrStatus::Confirmed)
      } else {
        Err(Error::auth(
          "login confirmed but Zhihu issued no z_c0 cookie",
        ))
      }
    }
    _ => Ok(QrStatus::Waiting),
  }
}

/// Cookies some scan_info responses carry in the body instead of `Set-Cookie`.
fn absorb_cookies(api: &Api, v: &Value) {
  if let Some(raw) = v
    .first_str(&["cookie", "cookies"])
    .filter(|c| c.contains("z_c0"))
  {
    for part in raw.split(';') {
      if let Some((k, val)) = part.trim().split_once('=')
        && !k.is_empty()
      {
        api.ctx.http.set_cookie(k.trim(), val.trim());
      }
    }
  }
  if let Some(z) = v.str("z_c0") {
    api.ctx.http.set_cookie("z_c0", &z);
  }
}
