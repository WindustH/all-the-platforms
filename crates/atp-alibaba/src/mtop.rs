//! The mtop h5 gateway both sites call (`<base>/<api>/<version>/`).
//!
//! A call is signed with `md5(token&t&appKey&data)`, `token` being the part
//! of the `_m_h5_tk` cookie before `_`. A call without a valid token is
//! refused with `FAIL_SYS_TOKEN_*` while the answer sets a fresh cookie, so
//! it is signed again. `ret[0]` tells how a call went: `SUCCESS::…`, or a
//! code [`check`] maps to an error. Walls (`RGV587_ERROR`) name the page that
//! lifts them in `data.url`: a login page, or a slider.

use atp_core::http::Method;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};
use md5::{Digest, Md5};

/// How a gateway takes `data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
  /// `POST` with `data` as a form field (Xianyu's web app).
  Form,
  /// `GET` with `data` in the query (Taobao's pages).
  Query,
}

/// One site's gateway.
#[derive(Debug, Clone, Copy)]
pub struct Gateway {
  pub base: &'static str,
  pub app_key: &'static str,
  pub style: Style,
  /// Query pairs every call of the site carries.
  pub query: &'static [(&'static str, &'static str)],
  pub referer: &'static str,
  /// The site's name in error messages.
  pub site: &'static str,
}

pub const GOOFISH: Gateway = Gateway {
  base: "https://h5api.m.goofish.com/h5",
  app_key: "34839810",
  style: Style::Form,
  query: &[
    ("accountSite", "xianyu"),
    ("sessionOption", "AutoLoginOnly"),
  ],
  referer: "https://www.goofish.com/",
  site: "Xianyu",
};

pub const TAOBAO: Gateway = Gateway {
  base: "https://h5api.m.taobao.com/h5",
  app_key: "12574478",
  style: Style::Query,
  query: &[],
  referer: "https://www.taobao.com/",
  site: "Taobao",
};

const TOKEN_COOKIE: &str = "_m_h5_tk";
/// Signing again twice covers an empty and then an expired token.
const ATTEMPTS: usize = 3;

/// One call: the API, its version, its `data` and what else its page adds.
pub struct Call<'a> {
  pub api: &'a str,
  pub version: &'a str,
  /// The exact text that is signed and sent.
  pub data: String,
  pub query: Vec<(&'a str, String)>,
  pub headers: Vec<(&'a str, String)>,
}

impl<'a> Call<'a> {
  pub fn new(api: &'a str, version: &'a str, data: &Value) -> Self {
    Self {
      api,
      version,
      data: data.to_string(),
      query: Vec::new(),
      headers: Vec::new(),
    }
  }

  pub fn query(mut self, key: &'a str, value: impl Into<String>) -> Self {
    self.query.push((key, value.into()));
    self
  }

  pub fn header(mut self, name: &'a str, value: impl Into<String>) -> Self {
    self.headers.push((name, value.into()));
    self
  }
}

pub fn sign(token: &str, t: &str, app_key: &str, data: &str) -> String {
  format!("{:x}", Md5::digest(format!("{token}&{t}&{app_key}&{data}")))
}

/// The signing token: `_m_h5_tk` up to its `_`, empty without the cookie.
pub fn token(ctx: &Ctx) -> String {
  ctx
    .http
    .cookie(TOKEN_COOKIE)
    .and_then(|c| c.split('_').next().map(str::to_owned))
    .unwrap_or_default()
}

/// Send a call, signed again with the fresh token while the gateway says
/// the token is missing or expired, and return its `data`.
pub async fn call(ctx: &Ctx, gateway: &Gateway, call: &Call<'_>) -> Result<Value> {
  let mut answer = Value::Null;
  for _ in 0..ATTEMPTS {
    answer = send(ctx, gateway, call, &token(ctx)).await?;
    if !is_token_error(&code(&answer)) {
      break;
    }
    tracing::debug!("{}: {}; signing again", call.api, code(&answer));
  }
  check(gateway, answer)
}

/// A call signed with an empty token, made for the cookies its answer sets
/// (`_m_h5_tk`, `cookie2`); the answer itself is not checked.
pub async fn touch(ctx: &Ctx, gateway: &Gateway, api: &str) -> Result<()> {
  let call = Call::new(api, "1.0", &json!({}));
  send(ctx, gateway, &call, "").await.map(drop)
}

async fn send(ctx: &Ctx, gateway: &Gateway, call: &Call<'_>, token: &str) -> Result<Value> {
  let t = jiff::Timestamp::now().as_millisecond().to_string();
  let sign = sign(token, &t, gateway.app_key, &call.data);
  let mut query: Vec<(&str, String)> = vec![
    ("jsv", "2.7.2".into()),
    ("appKey", gateway.app_key.into()),
    ("t", t),
    ("sign", sign),
    ("v", call.version.into()),
    ("type", "originaljson".into()),
    ("dataType", "json".into()),
    ("timeout", "20000".into()),
    ("api", call.api.into()),
  ];
  query.extend(gateway.query.iter().map(|(k, v)| (*k, (*v).to_owned())));
  query.extend(call.query.iter().cloned());
  let url = format!("{}/{}/{}/", gateway.base, call.api, call.version);
  let method = match gateway.style {
    Style::Form => Method::POST,
    Style::Query => {
      query.push(("data", call.data.clone()));
      Method::GET
    }
  };
  let mut req = ctx
    .http
    .request(method, url)
    .queries(query)
    .header("referer", gateway.referer);
  for (name, value) in &call.headers {
    req = req.header(name, value);
  }
  if gateway.style == Style::Form {
    req = req.form([("data", call.data.as_str())]);
  }
  parse(&req.send().await?.text())
}

/// An answer as JSON, also when it comes wrapped as JSONP (`mtopjsonp1({…})`).
pub fn parse(text: &str) -> Result<Value> {
  let text = text.trim();
  let inner = match (text.find('('), text.rfind(')')) {
    (Some(open), Some(close)) if !text.starts_with('{') && open < close => &text[open + 1..close],
    _ => text,
  };
  serde_json::from_str(inner).map_err(|_| {
    Error::upstream(format!(
      "the mtop gateway answered with something else than JSON: {}",
      atp_core::text::truncate(&atp_core::text::one_line(text), 120)
    ))
  })
}

/// `ret[0]` of an answer: `SUCCESS::调用成功`, `FAIL_SYS_TOKEN_EXOIRED::令牌过期` …
pub fn code(answer: &Value) -> String {
  answer.str("ret.0").unwrap_or_default()
}

fn is_token_error(code: &str) -> bool {
  [
    "FAIL_SYS_TOKEN_EXOIRED",
    "FAIL_SYS_TOKEN_EMPTY",
    "FAIL_SYS_TOKEN_ILLEGAL",
    "令牌过期",
    "令牌为空",
  ]
  .iter()
  .any(|c| code.contains(c))
}

/// The `data` of a successful answer, or the error its `ret` stands for.
pub fn check(gateway: &Gateway, answer: Value) -> Result<Value> {
  let code = code(&answer);
  if code.starts_with("SUCCESS") {
    return Ok(match answer {
      Value::Object(mut map) => map.remove("data").unwrap_or(Value::Null),
      _ => Value::Null,
    });
  }
  Err(failure(gateway, &code, answer.str("data.url")))
}

fn failure(gateway: &Gateway, code: &str, url: Option<String>) -> Error {
  let site = gateway.site;
  let has = |words: &[&str]| words.iter().any(|w| code.contains(w));
  let message = if code.is_empty() {
    format!("{site} answered without a result code")
  } else {
    format!("{site}: {code}")
  };
  let login_wall = url.as_deref().is_some_and(|u| {
    (u.contains("mini_login") || u.contains("member/login")) && !u.contains("punish")
  });
  if has(&[
    "SESSION_EXPIRED",
    "FAIL_SYS_SESSION",
    "NOT_LOGIN",
    "ERR_SID_INVALID",
    "FAIL_SYS_LOGIN",
  ]) || has(&["未登录", "请登录", "请重新登录"])
    || (has(&["RGV587"]) && login_wall)
  {
    return Error::auth(format!("{site} wants a login ({code})"));
  }
  if has(&[
    "RGV587",
    "FAIL_SYS_USER_VALIDATE",
    "FAIL_SYS_ILLEGAL_ACCESS",
  ]) {
    let hint = match url {
      Some(url) => format!(
        "open {url} in a browser and pass the check, or wait a few minutes; then import the browser's cookies again"
      ),
      None => {
        "wait a few minutes, or open the site in a browser once and import its cookies again".into()
      }
    };
    return Error::new(ErrorCode::VerificationRequired, message).with_hint(hint);
  }
  if has(&[
    "FAIL_SYS_TRAFFIC_LIMIT",
    "FLOWLIMIT",
    "FAIL_SYS_HSF_THROTTLE",
  ]) {
    return Error::new(ErrorCode::RateLimited, message);
  }
  if has(&["NOT_FOUND", "NOT_EXIST", "ITEM_DELETED", "不存在", "已删除"]) {
    return Error::not_found(message);
  }
  Error::upstream(message)
}
