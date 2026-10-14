//! JD transport: the color gateway `api.m.jd.com` (`functionId` + `appid` +
//! JSON `body`, signed with h5st where the page signs), pages on
//! `mall.jd.com` and `passport.jd.com`, and the mapping of JD's refusals:
//! risk checks (code 605, `x-rp-content`, redirects to the risk handler)
//! become `verification_required` with the check page, empty 403s
//! `rate_limited`, login walls `not_authenticated`. [`Api`] owns the
//! context.

use atp_core::http::Resp;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

use crate::sign::{self, Page, Signer};

pub const WWW: &str = "https://www.jd.com";
const GATEWAY: &str = "https://api.m.jd.com";
pub const SEARCH_ORIGIN: &str = "https://search.jd.com";
pub const ITEM_ORIGIN: &str = "https://item.jd.com";
/// Cookies of a logged-in desktop session (the mobile site's `pt_key` and
/// `pt_pin` are accepted by the calls as well).
pub const LOGIN_COOKIES: &[&str] = &["thor", "pin"];
/// Delivery area when the session has none (`ipLoc-djd`): Beijing.
const DEFAULT_AREA: &str = "1_2800_55812_0";
const RISK_HINT: &str = "JD's risk control refused this client; open www.jd.com in a browser, \
  then run `atp jd login --browser` so atp uses that browser's identity, or try again later";

/// The page a gateway call claims to come from.
pub struct From {
  pub origin: &'static str,
  pub referer: String,
  /// `x-referer-page` and `x-rp-client`, sent by item pages.
  pub page: Option<(String, &'static str)>,
}

impl From {
  pub fn search(keyword: &str) -> Self {
    From {
      origin: SEARCH_ORIGIN,
      referer: search_url(keyword),
      page: None,
    }
  }

  pub fn item(sku: &str) -> Self {
    From {
      origin: ITEM_ORIGIN,
      referer: format!("{ITEM_ORIGIN}/"),
      page: Some((item_url(sku), "h5_2.2.0")),
    }
  }
}

/// One gateway call. Parameters go out in `order`; the rest follow.
/// `params` add to the ones every call has or replace them.
pub struct Call {
  pub function: &'static str,
  pub appid: &'static str,
  /// `/`, `/api` or `/client.action`.
  pub path: &'static str,
  /// The h5st signing app id; `None` for calls pages send unsigned.
  pub sign: Option<&'static str>,
  pub body: Option<String>,
  pub params: Vec<(&'static str, String)>,
  pub from: From,
  /// POST the parameters as a form instead of a GET query.
  pub form: bool,
  /// Whether the call carries a time `t` (signed calls always do).
  pub time: bool,
  pub order: &'static [&'static str],
}

pub struct Api {
  pub ctx: Ctx,
  signer: Signer,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      signer: Signer::default(),
    }
  }

  /// The logged-in account's pin.
  pub fn pin(&self) -> Option<String> {
    let http = &self.ctx.http;
    http
      .cookie("pin")
      .or_else(|| http.cookie("pt_pin"))
      .map(|p| crate::parse::unescape(&p))
  }

  pub fn require_login(&self) -> Result<()> {
    let http = &self.ctx.http;
    let ticket = http.has_cookie("thor") || http.has_cookie("pt_key");
    if ticket && self.pin().is_some() {
      return Ok(());
    }
    Err(
      Error::auth("not logged in to JD (no thor / pt_key and pin cookies)")
        .with_hint(self.ctx.login_hint()),
    )
  }

  /// Delivery area prices and stock are given for (`省_市_区_镇`).
  pub fn area(&self) -> String {
    self
      .ctx
      .http
      .cookie("ipLoc-djd")
      .map(|a| a.replace('-', "_"))
      .and_then(|a| a.split('.').next().map(str::to_owned))
      .filter(|a| !a.is_empty())
      .unwrap_or_else(|| DEFAULT_AREA.to_owned())
  }

  /// A gateway call; its JSON answer.
  pub async fn call(&self, call: Call) -> Result<Value> {
    let (params, local) = self.params(&call).await?;
    let url = format!("{GATEWAY}{}", call.path);
    let http = &self.ctx.http;
    let mut req = if call.form {
      http.post(&url).form(params)
    } else {
      http.get(&url).queries(params)
    };
    req = req
      .header("accept", "application/json, text/plain, */*")
      .header("origin", call.from.origin)
      .header("referer", &call.from.referer)
      .header("sec-fetch-site", "same-site")
      .header("sec-fetch-mode", "cors")
      .header("sec-fetch-dest", "empty");
    if let Some((page, client)) = &call.from.page {
      req = req
        .header("x-referer-page", page)
        .header("x-rp-client", client);
    }
    let resp = req.send().await?;
    check(&resp, &call).map_err(|e| match local {
      Some(reason) if e.code != ErrorCode::NotAuthenticated => e.with_hint(format!(
        "atp signed with a local token because request_algo failed ({reason})"
      )),
      _ => e,
    })
  }

  /// The parameters in the page's order, signed when the page signs.
  async fn params(&self, call: &Call) -> Result<(Vec<(String, String)>, Option<String>)> {
    let http = &self.ctx.http;
    let mut params: Vec<(&str, String)> = vec![
      ("appid", call.appid.to_owned()),
      ("functionId", call.function.to_owned()),
      ("client", "pc".to_owned()),
      ("clientVersion", "1.0.0".to_owned()),
    ];
    if let Some(body) = &call.body {
      params.push(("body", body.clone()));
    }
    if call.time || call.sign.is_some() {
      params.push(("t", jiff::Timestamp::now().as_millisecond().to_string()));
    }
    if let Some(uuid) = http.cookie("__jdu") {
      params.push(("uuid", uuid));
    }
    // The call's own values win (`client`, `clientVersion`, `uuid` ...).
    for (name, value) in &call.params {
      match params.iter_mut().find(|(k, _)| k == name) {
        Some(slot) => slot.1 = value.clone(),
        None => params.push((*name, value.clone())),
      }
    }
    let mut local = None;
    if let Some(app_id) = call.sign {
      params.push(("loginType", "3".to_owned()));
      if let Some(eid) = http.cookie("3AB9D23F7A4B3CSS") {
        params.push(("x-api-eid-token", eid));
      }
      let page = Page {
        url: call
          .from
          .page
          .as_ref()
          .map_or(&call.from.referer, |(p, _)| p),
        origin: call.from.origin,
      };
      let signed = self
        .signer
        .sign(&self.ctx, app_id, &page, &signed_values(&params))
        .await?;
      params.push(("h5st", signed.h5st));
      local = signed.local;
    }
    let mut ordered: Vec<(String, String)> = Vec::new();
    for name in call.order {
      if let Some(i) = params.iter().position(|(k, _)| k == name) {
        let (k, v) = params.remove(i);
        ordered.push((k.to_owned(), v));
      }
    }
    ordered.extend(params.into_iter().map(|(k, v)| (k.to_owned(), v)));
    Ok((ordered, local))
  }

  /// A page of a JD site, refusing risk-check redirects.
  pub async fn page(&self, url: &str) -> Result<String> {
    let resp = self
      .ctx
      .http
      .get(url)
      .header(
        "accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
      )
      .header("sec-fetch-mode", "navigate")
      .header("sec-fetch-dest", "document")
      .send()
      .await?;
    if let Some(e) = risk_redirect(&resp) {
      return Err(e);
    }
    Ok(resp.check()?.text())
  }
}

/// What h5st covers: these names, with the body as its SHA-256.
fn signed_values(params: &[(&str, String)]) -> Vec<(&'static str, String)> {
  const SIGNED: &[&str] = &[
    "appid",
    "functionId",
    "body",
    "client",
    "clientVersion",
    "t",
  ];
  SIGNED
    .iter()
    .filter_map(|&name| {
      let value = params.iter().find(|(k, _)| *k == name)?.1.clone();
      let value = if name == "body" {
        sign::crypto::body_hash(&value)
      } else {
        value
      };
      Some((name, value))
    })
    .collect()
}

/// Map JD's refusals; the JSON answer otherwise.
fn check(resp: &Resp, call: &Call) -> Result<Value> {
  let function = call.function;
  let referer = call
    .from
    .page
    .as_ref()
    .map_or(&call.from.referer, |(page, _)| page);
  if let Some(e) = risk_redirect(resp).or_else(|| disposal_header(resp, referer)) {
    return Err(e);
  }
  if resp.status.as_u16() == 403 && resp.body.iter().all(u8::is_ascii_whitespace) {
    return Err(
      Error::new(
        ErrorCode::RateLimited,
        format!("JD's edge refused {function} (HTTP 403 without a body)"),
      )
      .with_hint(RISK_HINT),
    );
  }
  let v = resp.value().map_err(|e| match resp.status.is_success() {
    true => e,
    false => atp_core::http::status_error(resp.status, &resp.text()),
  })?;
  if let Some(e) = disposal(&v, referer) {
    return Err(e);
  }
  if login_wall(&v) {
    return Err(Error::auth(format!("JD wants a login for {function}")));
  }
  let code = v
    .get("code")
    .map(|c| c.to_string().trim_matches('"').to_owned());
  let failed = v.bool("success") == Some(false)
    || (v.get("echo").is_some() && code.as_deref().is_some_and(|c| c != "0"));
  if failed {
    let message = v
      .first_str(&["message", "msg", "resultMsg", "echo"])
      .unwrap_or_else(|| "request failed".into());
    return Err(Error::upstream(format!(
      "JD {function}: {message} (code {})",
      code.unwrap_or_default()
    )));
  }
  Ok(v)
}

/// A redirect to the risk handler (`cfe.m.jd.com/privatedomain/risk_handler`).
fn risk_redirect(resp: &Resp) -> Option<Error> {
  (resp.url.contains("risk_handler") || resp.url.contains("cfe.m.jd.com")).then(|| {
    Error::new(ErrorCode::VerificationRequired, "JD asks for a risk check")
      .with_url(&resp.url)
      .with_hint(RISK_HINT)
  })
}

/// A risk disposal sent only in the `x-rp-content` header (base64 JSON).
fn disposal_header(resp: &Resp, referer: &str) -> Option<Error> {
  let raw = resp.header("x-rp-content")?;
  let text = base64url(raw)?;
  let v: Value = serde_json::from_slice(&text).ok()?;
  Some(disposal(&v, referer).unwrap_or_else(|| {
    Error::new(
      ErrorCode::VerificationRequired,
      "JD's risk control stopped the request",
    )
    .with_hint(RISK_HINT)
  }))
}

/// Code 605 or a `disposal` in the answer: a check to pass in a browser,
/// on the risk page the web app would open.
fn disposal(v: &Value, referer: &str) -> Option<Error> {
  let code = v
    .get("code")
    .map(|c| c.to_string().trim_matches('"').to_owned());
  let found = v.get("disposal").is_some_and(|d| !d.is_null()) || code.as_deref() == Some("605");
  if !found {
    return None;
  }
  let content: Value = v
    .str("disposal.evContent")
    .and_then(|c| serde_json::from_str(&c).ok())
    .unwrap_or_default();
  let mut error = Error::new(
    ErrorCode::VerificationRequired,
    "JD asks for a verification (code 605)",
  )
  .with_hint(RISK_HINT);
  if let Some(page) = content.str("evUrl") {
    let mut url = url::Url::parse(&page).ok()?;
    url.query_pairs_mut().extend_pairs([
      ("returnurl", referer.to_owned()),
      ("rqhost", GATEWAY.to_owned()),
      ("rpid", v.str("disposal.rpId").unwrap_or_default()),
      (
        "evtype",
        content.str("evType").unwrap_or_else(|| "2".into()),
      ),
      ("evapi", content.str("evApi").unwrap_or_default()),
      ("source", "1".into()),
      ("forceCurrentView", "1".into()),
      ("evsid", content.str("evSid").unwrap_or_default()),
    ]);
    error = error.with_url(url.as_str());
  }
  Some(error)
}

fn login_wall(v: &Value) -> bool {
  let code = v
    .get("code")
    .map(|c| c.to_string().trim_matches('"').to_owned());
  let message = v
    .first_str(&["message", "msg", "resultMsg", "echo"])
    .unwrap_or_default();
  code.as_deref() == Some("3")
    || v.i64("resultCode") == Some(-100)
    || message.contains("未登录")
    || message.contains("pin is null")
}

/// Base64 or base64url, padded or not.
fn base64url(text: &str) -> Option<Vec<u8>> {
  use base64::Engine;
  let text: String = text
    .trim()
    .chars()
    .filter(|&c| c != '=')
    .map(|c| match c {
      '-' => '+',
      '_' => '/',
      c => c,
    })
    .collect();
  base64::engine::general_purpose::STANDARD_NO_PAD
    .decode(text)
    .ok()
}

pub fn item_url(sku: &str) -> String {
  format!("{ITEM_ORIGIN}/{sku}.html")
}

pub fn search_url(keyword: &str) -> String {
  format!(
    "{SEARCH_ORIGIN}/Search?keyword={}&enc=utf-8",
    atp_core::text::encode_uri_component(keyword)
  )
}

pub fn shop_url(shop: &str) -> String {
  format!("https://mall.jd.com/index-{shop}.html")
}

/// `jfs/t1/…` image paths on JD's CDN.
pub fn image_url(path: &str) -> String {
  let path = path.trim();
  if let Some(rest) = path.strip_prefix("//") {
    format!("https://{rest}")
  } else if path.starts_with("http") {
    path.replacen("http://", "https://", 1)
  } else {
    format!(
      "https://img14.360buyimg.com/n1/{}",
      path.trim_start_matches('/')
    )
  }
}
