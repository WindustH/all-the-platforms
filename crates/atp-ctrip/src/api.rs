//! Ctrip transport. Data comes from the SOA gateway
//! (`m.ctrip.com/restapi/soa2/<service>/<operation>`, JSON POSTs with a
//! visitor id and a `head`) and from the server-rendered pages of the
//! sites (`you`, `hotels`, `trains`, `m.ctrip.com`) and a JSON service of
//! `flights.ctrip.com` (airport cities).
//!
//! The gateway checks the `Origin` of each service against the sites that
//! call it, so every call names the site it comes from ([`Site`]). Nothing
//! is signed: atp sends no `phantom-token`, `w-payload-source` or RMS
//! fingerprint, and calls that want one are left out.

use std::time::Duration;

use atp_core::http::Resp;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};
use rand::Rng;

pub const M: &str = "https://m.ctrip.com";
pub const YOU: &str = "https://you.ctrip.com";
pub const HOTELS: &str = "https://hotels.ctrip.com";
pub const TRAINS: &str = "https://trains.ctrip.com";
pub const FLIGHTS: &str = "https://flights.ctrip.com";
const GATEWAY: &str = "https://m.ctrip.com/restapi/soa2";

/// Cookies of a logged-in session.
pub const LOGIN_COOKIES: &[&str] = &["cticket"];
/// Where atp keeps the visitor id between runs.
const CID_KEY: &str = "ctrip-cid";
const CID_TTL: Duration = Duration::from_secs(180 * 24 * 3600);

/// Chrome on Android, for the mobile pages (the same browser as atp's
/// client, on a phone).
const MOBILE_UA: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 \
  (KHTML, like Gecko) Chrome/149.0.0.0 Mobile Safari/537.36";

const BLOCKED_HINT: &str = "Ctrip's firewall blocks busy addresses for an hour or more; \
  wait before trying again (atp does not retry)";
const CHECK_HINT: &str = "pass the check in the browser atp opens (or open the page yourself), \
  then try again; keep requests sparse";

/// The site a request comes from: the gateway checks it as `Origin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
  Mobile,
  You,
  Hotels,
  Trains,
  Flights,
}

impl Site {
  pub fn origin(self) -> &'static str {
    match self {
      Site::Mobile => M,
      Site::You => YOU,
      Site::Hotels => HOTELS,
      Site::Trains => TRAINS,
      Site::Flights => FLIGHTS,
    }
  }

  fn mobile(self) -> bool {
    matches!(self, Site::Mobile | Site::Trains)
  }
}

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  pub fn require_login(&self) -> Result<()> {
    self.ctx.require_login(LOGIN_COOKIES)
  }

  /// The visitor id (`cid`, `_fxpcqlniredt`): the session's `GUID` cookie,
  /// else one atp made once and keeps.
  pub fn cid(&self) -> String {
    if let Some(guid) = self
      .ctx
      .http
      .cookie("GUID")
      .filter(|g| g.len() >= 16 && g.bytes().all(|b| b.is_ascii_digit()))
    {
      return guid;
    }
    if let Some(cid) = self.ctx.store.cache_get::<String>(CID_KEY, CID_TTL) {
      return cid;
    }
    let mut rng = rand::rng();
    let tail: String = (0..15)
      .map(|_| char::from(b'0' + rng.random_range(0..10u8)))
      .collect();
    let cid = format!("09031{tail}");
    self.ctx.store.cache_put(CID_KEY, &cid);
    cid
  }

  /// The `head` most services take, as the H5 site sends it.
  pub fn head(&self) -> Value {
    json!({
      "cid": self.cid(), "ctok": "", "cver": "1.0", "lang": "01", "sid": "8888",
      "syscode": "09", "auth": "", "xsid": "", "extension": []
    })
  }

  /// The `head` of the hotel services, for the page `page_id`.
  pub fn hotel_head(&self, page_id: &str) -> Value {
    let cid = self.cid();
    json!({
      "platform": "PC", "cver": "0", "cid": cid, "bu": "HBU", "group": "ctrip",
      "aid": "", "sid": "", "ouid": "", "locale": "zh-CN", "timezone": "8",
      "currency": "CNY", "pageId": page_id, "vid": cid, "guid": cid, "isSSR": false
    })
  }

  /// POST `body` to a gateway service (`13444/json/getCommentCollapseList`)
  /// as `site` calls it; a `head` is added when the body has none.
  pub async fn soa(&self, path: &str, site: Site, mut body: Value) -> Result<Value> {
    if body.get("head").is_none() {
      body["head"] = self.head();
    }
    let cid = self.cid();
    let origin = site.origin();
    let from = format!("{origin}/");
    let resp = self
      .ctx
      .http
      .post(format!("{GATEWAY}/{path}"))
      .query("_fxpcqlniredt", &cid)
      .query("x-traceID", trace_id(&cid))
      .header("accept", "application/json")
      .header("origin", origin)
      .header("referer", &from)
      .header("cookieorigin", origin)
      .header("x-ctx-locale", "zh-CN")
      .header("x-ctx-currency", "CNY")
      .json(&body)
      .send()
      .await?;
    if let Some(e) = refusal(&self.ctx, &resp, &from) {
      return Err(e);
    }
    let v = resp.check()?.value()?;
    match refused(&self.ctx, &v, &from) {
      Some(e) => Err(e),
      None => Ok(v),
    }
  }

  /// A JSON answer of one of the sites' own services (GET).
  pub async fn get(&self, url: &str, site: Site) -> Result<Value> {
    let resp = self
      .ctx
      .http
      .get(url)
      .header("accept", "application/json")
      .header("referer", format!("{}/", site.origin()))
      .send()
      .await?;
    if let Some(e) = refusal(&self.ctx, &resp, url) {
      return Err(e);
    }
    resp.check()?.value()
  }

  /// A page of one of the sites, as a browser opens it.
  pub async fn page(&self, url: &str, site: Site) -> Result<String> {
    let mut req = self
      .ctx
      .http
      .get(url)
      .header(
        "accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
      )
      .header("accept-language", "zh-CN,zh;q=0.9")
      .header("referer", format!("{}/", site.origin()))
      .header("sec-fetch-mode", "navigate")
      .header("sec-fetch-dest", "document");
    if site.mobile() {
      req = req
        .header("user-agent", MOBILE_UA)
        .header("sec-ch-ua-mobile", "?1")
        .header("sec-ch-ua-platform", "\"Android\"");
    }
    let resp = req.send().await?;
    if let Some(e) = refusal(&self.ctx, &resp, url) {
      return Err(e);
    }
    Ok(resp.check()?.text())
  }
}

/// `<cid>-<epoch ms>-<7 digits>`, as the pages name their requests.
fn trace_id(cid: &str) -> String {
  let n: u32 = rand::rng().random_range(1_000_000..10_000_000);
  format!("{cid}-{}-{n}", jiff::Timestamp::now().as_millisecond())
}

/// How the gateway or a site says no, by status and where it led.
fn refusal(ctx: &Ctx, resp: &Resp, page: &str) -> Option<Error> {
  let status = resp.status.as_u16();
  if status == 430 {
    return Some(
      Error::new(
        ErrorCode::IpBlocked,
        "Ctrip's firewall blocked this address (HTTP 430)",
      )
      .with_hint(BLOCKED_HINT),
    );
  }
  let url = resp.url.to_ascii_lowercase();
  if url.contains("passport.ctrip.com") || url.contains("/user/login") {
    return Some(login(ctx, "Ctrip sent this request to its login page"));
  }
  if url.contains("captcha") || url.contains("/verify") {
    return Some(check(&format!("Ctrip shows a check ({})", resp.url), page));
  }
  if status != 403 {
    return None;
  }
  let code = resp.value().ok().and_then(|v| v.str("code"));
  Some(match code.as_deref() {
    Some("11005") => Error::internal(
      "Ctrip refused the request's origin (11005 Unauthorized Origin); this is a bug in atp",
    ),
    Some("11010") => Error::new(
      ErrorCode::PermissionDenied,
      "Ctrip refuses this call from web clients (11010 Operation Forbidden)",
    ),
    _ => check("Ctrip's gateway refused the request (HTTP 403)", page),
  })
}

/// Refusals written in a JSON answer: login walls, risk checks, failures.
fn refused(ctx: &Ctx, v: &Value, page: &str) -> Option<Error> {
  if v.bool("needUserLogin") == Some(true) || v.bool("data.needUserLogin") == Some(true) {
    return Some(login(ctx, "Ctrip wants a login for this"));
  }
  if v.bool("showAuthCode") == Some(true) || v.bool("data.showAuthCode") == Some(true) {
    return Some(check("Ctrip wants a captcha before this request", page));
  }
  match v.i64("data.htlSpiderActionErrorCode") {
    Some(4030) => return Some(login(ctx, "Ctrip shows hotel prices after a login (4030)")),
    Some(203) => {
      return Some(check(
        "Ctrip's hotel risk control stopped this request (203)",
        page,
      ));
    }
    Some(code) if code != 0 => {
      return Some(Error::upstream(format!(
        "Ctrip's hotel service refused the request ({code})"
      )));
    }
    _ => {}
  }
  failure(ctx, v)
}

/// A `ResponseStatus` with `Ack: Failure`.
fn failure(ctx: &Ctx, v: &Value) -> Option<Error> {
  let status = ["ResponseStatus", "responseStatus"]
    .iter()
    .map(|k| v.at(k))
    .find(|s| s.is_object())?;
  if status.str("Ack").as_deref() != Some("Failure") {
    return None;
  }
  let error = status.at("Errors.0");
  let code = error.str("ErrorCode").unwrap_or_default();
  let message = error.str("Message").unwrap_or_default();
  let auth = ["Token为空", "cookie auth is null", "未登录"];
  if code.contains("AccountsMobileRequestFilterException")
    || auth.iter().any(|w| message.contains(w))
  {
    return Some(login(
      ctx,
      format!("Ctrip wants a login for this ({message})"),
    ));
  }
  Some(Error::upstream(format!("Ctrip answered {code}: {message}")))
}

fn login(ctx: &Ctx, message: impl Into<String>) -> Error {
  Error::auth(message).with_hint(ctx.login_hint())
}

fn check(message: &str, page: &str) -> Error {
  Error::new(ErrorCode::VerificationRequired, message)
    .with_url(page)
    .with_hint(CHECK_HINT)
}
