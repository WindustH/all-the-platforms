//! Visitor identity: the `ttwid` cookie (registered with ByteDance's union
//! service) and the `s_v_web_id` fingerprint. Without them the web API
//! answers empty bodies. A visitor `ttwid` is cached for a week so every run
//! does not register a new device.

use std::time::Duration;

use crate::api::Api;

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
pub async fn ensure(api: &Api) {
  let http = &api.ctx.http;
  if http.has_cookie("ttwid") && http.has_cookie("s_v_web_id") {
    return;
  }
  if let Some(v) = api.ctx.store.cache_get::<Visitor>(CACHE_KEY, CACHE_TTL) {
    if !http.has_cookie("ttwid") {
      http.set_cookie("ttwid", &v.ttwid);
    }
    if !http.has_cookie("s_v_web_id") {
      http.set_cookie("s_v_web_id", &v.fp);
    }
    return;
  }
  if !http.has_cookie("s_v_web_id") {
    let fp = crate::sign::verify_fp(jiff::Timestamp::now().as_millisecond());
    http.set_cookie("s_v_web_id", &fp);
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
    api.ctx.store.cache_put(CACHE_KEY, &Visitor { ttwid, fp });
  }
}
