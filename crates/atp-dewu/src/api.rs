//! Dewu transport. Three routes, all without a user login:
//!
//! - the web calls of www.dewu.com to `app.dewu.com` (JSON POSTs and
//!   GETs carrying the md5 `sign` of [`sign::web`]), sent as the page
//!   sends them minus what its `pceptor.js` adds (device-fingerprint
//!   headers atp does not make);
//! - the community share pages on `m.dewu.com`, server-rendered with the
//!   post in `__NEXT_DATA__`, no signature;
//! - the Open Platform (`Open`, in [`crate::open`]).
//!
//! Every request goes out without cookies. Refusals map onto atp errors
//! with the page to open: a captcha demand (code 485) is
//! `verification_required`, a refusal (`请求已拒绝`, code 7999, which the
//! site answers to calls without its device headers) `upstream_error`.

use std::sync::LazyLock;

use atp_core::http::Resp;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};
use regex::Regex;
use serde_json::Map;

use crate::open::Open;
use crate::sign;

pub const WWW: &str = "https://www.dewu.com";
const GATEWAY: &str = "https://app.dewu.com";
pub const COMMUNITY: &str = "https://www.dewu.com/community";
const SHARE: &str = "https://m.dewu.com/rn-activity/community-share";
/// A phone browser: the share pages are made for phones.
const MOBILE_UA: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, \
  like Gecko) Chrome/149.0.0.0 Mobile Safari/537.36";
const CHECK_HINT: &str = "Dewu wants a check before this call; the site also sends device headers \
  from its pceptor.js, which atp does not make. Open the page in a browser, or try again later";
const REFUSED_HINT: &str = "Dewu refuses this call from clients without the device headers of its \
  pceptor.js, which atp does not make (it answers the same to addresses it blocks); open the page \
  in a browser";

static NEXT_DATA: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r#"(?s)<script[^>]*id="__NEXT_DATA__"[^>]*>(.*?)</script>"#).expect("valid")
});

pub struct Api {
  pub ctx: Ctx,
  pub open: Open,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      open: Open::from_env(),
    }
  }

  /// A signed JSON POST of the web pages; `page` is the www.dewu.com page
  /// it is made from (opened for checks), `headers` the module's own.
  pub async fn post(
    &self,
    path: &str,
    body: Value,
    page: &str,
    headers: &[(&str, &str)],
  ) -> Result<Value> {
    let mut params = object(body);
    let sign = sign::web(&mut params);
    let mut signed = Map::new();
    signed.insert("sign".into(), sign.into());
    signed.extend(params);
    let mut req = self
      .web(self.ctx.http.post(format!("{GATEWAY}{path}")))
      .json_text(Value::Object(signed).to_string());
    for (name, value) in headers {
      req = req.header(name, value);
    }
    check(req.send().await?, path, page)
  }

  /// A signed GET of the web pages (`sign` first, then the parameters).
  pub async fn get(&self, path: &str, params: Value, page: &str) -> Result<Value> {
    let mut params = object(params);
    let sign = sign::web(&mut params);
    let mut query = vec![("sign".to_owned(), sign)];
    query.extend(params.iter().map(|(k, v)| (k.clone(), query_value(v))));
    let req = self
      .web(self.ctx.http.get(format!("{GATEWAY}{path}")))
      .queries(query);
    check(req.send().await?, path, page)
  }

  fn web<'a>(&self, req: atp_core::http::Req<'a>) -> atp_core::http::Req<'a> {
    req
      .no_cookies()
      .header("origin", WWW)
      .header("referer", format!("{WWW}/"))
      .header("sec-fetch-site", "same-site")
      .header("sec-fetch-mode", "cors")
      .header("sec-fetch-dest", "empty")
  }

  /// The `__NEXT_DATA__` of a community share page.
  pub async fn share_page(&self, trend: &str) -> Result<Value> {
    let url = share_url(trend);
    let resp = self
      .ctx
      .http
      .get(&url)
      .no_cookies()
      .header("user-agent", MOBILE_UA)
      .header("sec-ch-ua-mobile", "?1")
      .header("sec-ch-ua-platform", r#""Android""#)
      .header(
        "accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
      )
      .header("sec-fetch-site", "none")
      .header("sec-fetch-mode", "navigate")
      .header("sec-fetch-dest", "document")
      .send()
      .await?
      .check()?;
    let html = resp.text();
    let data = NEXT_DATA
      .captures(&html)
      .ok_or_else(|| Error::upstream(format!("the share page of {trend} carried no post data")))?;
    Ok(serde_json::from_str(&data[1])?)
  }
}

/// A JSON object of parameters (anything else signs as nothing).
fn object(value: Value) -> Map<String, Value> {
  match value {
    Value::Object(map) => map,
    _ => Map::new(),
  }
}

/// `URLSearchParams` text of a value.
fn query_value(value: &Value) -> String {
  match value {
    Value::String(s) => s.clone(),
    Value::Null => "NaN".into(),
    other => other.to_string(),
  }
}

/// The `data` envelope `{code | status: 200, msg, data}`, or the error.
fn check(resp: Resp, path: &str, page: &str) -> Result<Value> {
  let status = resp.status.as_u16();
  let v = match resp.value() {
    Ok(v) => v,
    Err(_) if matches!(status, 403 | 412 | 418 | 461 | 471) || resp.status.is_success() => {
      return Err(check_error(
        format!("Dewu answered {path} with HTTP {status} and no JSON"),
        page,
      ));
    }
    Err(_) => return Err(atp_core::http::status_error(resp.status, &resp.text())),
  };
  if v.i64("code") == Some(200) || v.i64("status") == Some(200) {
    return Ok(v);
  }
  Err(refusal(&v, path, page))
}

fn check_error(message: String, page: &str) -> Error {
  Error::new(ErrorCode::VerificationRequired, message)
    .with_url(page)
    .with_hint(CHECK_HINT)
}

/// The atp error for a failed envelope, by its wording.
fn refusal(v: &Value, path: &str, page: &str) -> Error {
  let code = v.first_str(&["code", "status"]).unwrap_or_default();
  let text = v
    .first_str(&["msg", "message", "error"])
    .unwrap_or_else(|| "request failed".into());
  let message = format!("Dewu {path}: {text} (code {code})");
  let has = |words: &[&str]| words.iter().any(|w| text.contains(w));
  if code == "485" || has(&["验证", "滑块", "captcha"]) {
    check_error(message, page)
  } else if code == "7999" || has(&["请求已拒绝", "拒绝访问", "非法请求"]) {
    Error::upstream(message)
      .with_url(page)
      .with_hint(REFUSED_HINT)
  } else if has(&["登录"]) {
    Error::auth(message).with_hint("Dewu wants a login here, which atp does not offer")
  } else if has(&["频繁", "限流", "稍后"]) {
    Error::new(ErrorCode::RateLimited, message)
  } else if has(&["签名", "sign"]) {
    Error::new(ErrorCode::SignatureError, message)
  } else if has(&["不存在", "下架"]) {
    Error::not_found(message)
  } else {
    Error::upstream(message).with_url(page)
  }
}

/// `https:` image and video links (some come protocol-relative or http).
pub fn media_url(url: &str) -> String {
  let url = url.trim();
  if let Some(rest) = url.strip_prefix("//") {
    format!("https://{rest}")
  } else {
    url.replacen("http://", "https://", 1)
  }
}

pub fn product_url(spu: &str) -> String {
  format!("{WWW}/product-detail.html?spuId={spu}&sourceName=pc")
}

pub fn share_url(trend: &str) -> String {
  format!("{SHARE}?trendId={trend}")
}
