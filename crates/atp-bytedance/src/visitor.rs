//! Visitor identity of ByteDance web sites: the `ttwid` cookie (registered
//! with ByteDance's union service, valid on every ByteDance domain) and the
//! `s_v_web_id` fingerprint. Without them the web APIs answer empty bodies.
//! A visitor `ttwid` is cached for a week so every run does not register a
//! new device.

use std::time::Duration;

use atp_core::Ctx;
use rand::Rng;

const REGISTER: &str = "https://ttwid.bytedance.com/ttwid/union/register/";
const REGISTER_BODY: &str = r#"{"region":"cn","aid":1768,"needFid":false,"service":"www.ixigua.com","migrate_info":{"ticket":"","source":"node"},"cbUrlProtocol":"https","union":true}"#;
const CACHE_KEY: &str = "visitor";
const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

#[derive(serde::Serialize, serde::Deserialize)]
struct Visitor {
  ttwid: String,
  fp: String,
}

/// Make sure the jar holds `ttwid` and `s_v_web_id`. Best effort: a failed
/// registration surfaces later as an empty answer.
pub async fn ensure(ctx: &Ctx) {
  let http = &ctx.http;
  if http.has_cookie("ttwid") && http.has_cookie("s_v_web_id") {
    return;
  }
  if let Some(v) = ctx.store.cache_get::<Visitor>(CACHE_KEY, CACHE_TTL) {
    if !http.has_cookie("ttwid") {
      http.set_cookie("ttwid", &v.ttwid);
    }
    if !http.has_cookie("s_v_web_id") {
      http.set_cookie("s_v_web_id", &v.fp);
    }
    return;
  }
  if !http.has_cookie("s_v_web_id") {
    http.set_cookie(
      "s_v_web_id",
      &verify_fp(jiff::Timestamp::now().as_millisecond()),
    );
  }
  if !http.has_cookie("ttwid") {
    let resp = http
      .post(REGISTER)
      .json_text(REGISTER_BODY)
      .no_throttle()
      .send()
      .await;
    if let Err(e) = resp {
      tracing::debug!("ttwid registration failed: {e}");
    }
  }
  if let (Some(ttwid), Some(fp)) = (http.cookie("ttwid"), http.cookie("s_v_web_id")) {
    ctx.store.cache_put(CACHE_KEY, &Visitor { ttwid, fp });
  }
}

/// `verify_<ms base36>_<uuid-like>`, as the web client mints `s_v_web_id`.
pub fn verify_fp(now_ms: i64) -> String {
  const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
  let mut ms = now_ms.max(0) as u64;
  let mut base36 = Vec::new();
  while ms > 0 {
    let d = (ms % 36) as u8;
    base36.push(if d < 10 { b'0' + d } else { b'a' + d - 10 });
    ms /= 36;
  }
  base36.reverse();
  let mut rng = rand::rng();
  let mut tail = [0u8; 36];
  for (i, slot) in tail.iter_mut().enumerate() {
    *slot = match i {
      8 | 13 | 18 | 23 => b'_',
      14 => b'4',
      _ => {
        let r = rng.random_range(0..DIGITS.len());
        DIGITS[if i == 19 { (r & 3) | 8 } else { r }]
      }
    };
  }
  format!(
    "verify_{}_{}",
    String::from_utf8_lossy(&base36),
    String::from_utf8_lossy(&tail)
  )
}

/// A random `msToken` for the first signed call, before a server hands one
/// out (in the `x-ms-token` response header).
pub fn ms_token() -> String {
  const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
  let mut rng = rand::rng();
  (0..107)
    .map(|_| char::from(CHARS[rng.random_range(0..CHARS.len())]))
    .collect()
}
