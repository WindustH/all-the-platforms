//! Group robot webhooks: text or Markdown, signed when the robot uses 加签.

use atp_core::{Result, Value, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::api::{Api, check_oapi};

pub async fn send(api: &Api, url: &str, secret: Option<&str>, text: &str) -> Result<()> {
  let mut req = api.ctx.http.post(url).no_cookies();
  if let Some(secret) = secret {
    let timestamp = jiff::Timestamp::now().as_millisecond();
    req = req
      .query("timestamp", timestamp)
      .query("sign", sign(secret, timestamp));
  }
  let body = if looks_markdown(text) {
    json!({
      "msgtype": "markdown",
      "markdown": { "title": text.lines().next().unwrap_or("message"), "text": text },
    })
  } else {
    json!({ "msgtype": "text", "text": { "content": text } })
  };
  let v: Value = req.json(&body).send().await?.value()?;
  check_oapi(v, "robot webhook").map(drop)
}

/// `base64(HmacSHA256(secret, "<timestamp>\n<secret>"))`.
fn sign(secret: &str, timestamp: i64) -> String {
  let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("any key length");
  mac.update(format!("{timestamp}\n{secret}").as_bytes());
  STANDARD.encode(mac.finalize().into_bytes())
}

fn looks_markdown(text: &str) -> bool {
  text.lines().any(|l| {
    let l = l.trim_start();
    l.starts_with('#') || l.starts_with("- ") || l.starts_with("> ") || l.contains("](")
  }) || text.contains("**")
}
