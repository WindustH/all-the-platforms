//! The visitor identity every Kuaishou page carries: the `did` the site
//! hands out with the first page, `didv`, the `kpf` / `kpn` / `clientid`
//! defaults and the webweapon `kwfv1` / `kwpsecproductname` pair. A
//! visitor's identity is cached for a week so runs without a login reuse it.

use std::time::Duration;

use rand::Rng;

use crate::api::{Api, DOC_ACCEPT, WWW};
use crate::sign::{hex, now_ms, weapon};

const CACHE_KEY: &str = "visitor";
const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
const DEFAULTS: &[(&str, &str)] = &[
  ("kpf", "PC_WEB"),
  ("kpn", "KUAISHOU_VISION"),
  ("clientid", "3"),
  ("kwpsecproductname", weapon::PRODUCT_WWW),
];

#[derive(serde::Serialize, serde::Deserialize)]
struct Visitor {
  did: String,
  didv: String,
  kwfv1: String,
}

/// Make sure the jar holds the visitor cookies. Cheap once they are there.
pub async fn ensure(api: &Api) {
  let http = &api.ctx.http;
  if http.has_cookie("did") && http.has_cookie("kwfv1") {
    return;
  }
  if !http.has_cookie("did") {
    visitor(api).await;
  }
  for (name, value) in DEFAULTS {
    if !http.has_cookie(name) {
      http.set_cookie(name, value);
    }
  }
  let did = http.cookie("did").unwrap_or_default();
  if !http.has_cookie("didv") {
    http.set_cookie("didv", &now_ms().to_string());
  }
  if !http.has_cookie("kwfv1") {
    let href = format!("{WWW}/new-reco");
    let kwfv1 = weapon::kwfv1(&href, &did, weapon::PRODUCT_WWW, now_ms());
    http.set_cookie("kwfv1", &kwfv1);
  }
  let visitor = Visitor {
    did,
    didv: http.cookie("didv").unwrap_or_default(),
    kwfv1: http.cookie("kwfv1").unwrap_or_default(),
  };
  api.ctx.store.cache_put(CACHE_KEY, &visitor);
}

/// A device id: the cached visitor's, or a new one from the home page.
async fn visitor(api: &Api) {
  let http = &api.ctx.http;
  if let Some(v) = api.ctx.store.cache_get::<Visitor>(CACHE_KEY, CACHE_TTL) {
    http.set_cookie("did", &v.did);
    http.set_cookie("didv", &v.didv);
    http.set_cookie("kwfv1", &v.kwfv1);
    return;
  }
  let resp = http
    .get(format!("{WWW}/new-reco"))
    .header("accept", DOC_ACCEPT)
    .header("sec-fetch-dest", "document")
    .header("sec-fetch-mode", "navigate")
    .send()
    .await;
  if let Err(e) = resp {
    tracing::debug!("visitor page failed: {e}");
  }
  if !http.has_cookie("did") {
    let bytes: [u8; 16] = rand::rng().random();
    http.set_cookie("did", &format!("web_{}", hex(&bytes)));
  }
}
