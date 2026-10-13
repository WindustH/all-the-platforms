//! The mp.weixin.qq.com backend: what the account owner's browser calls
//! after a QR login.
//!
//! Requests carry the session cookies plus `token=<digits>`, which the
//! backend hands out per session: it is read from where `GET /` redirects to
//! (`/cgi-bin/home?...&token=N`) and kept for the run. Errors come back as
//! HTTP 200 with `base_resp.ret` (200003 expired session, 200040 stale token,
//! 200013 rate limit), and many fields are JSON encoded once more inside a
//! string ([`nested`]).

use std::sync::LazyLock;

use regex::Regex;
use social_core::{Error, ErrorCode, Result, Value, ValueExt};

use crate::client::{Client, LOGIN_HINT};

pub const MP: &str = "https://mp.weixin.qq.com";
/// Cookie of a logged-in backend session.
pub const SESSION_COOKIE: &str = "slave_sid";
const FINGERPRINT_KEY: &str = "fingerprint";

static HOME_TOKEN: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"[?&]token=(\d+)").expect("valid"));

/// Query pairs, owned so callers can build them freely.
pub type Params = Vec<(&'static str, String)>;

impl Client {
  pub fn require_web(&self) -> Result<()> {
    if self.has_web() {
      Ok(())
    } else {
      Err(
        Error::auth("this needs the mp.weixin.qq.com backend: no QR login")
          .with_hint("run `social mp login` and scan the code with the WeChat of an account admin"),
      )
    }
  }

  /// The session's backend token: remembered for the run, else from the
  /// redirect of `GET /`.
  pub async fn web_token(&self) -> Result<String> {
    if let Some(t) = self.web_token.borrow().clone() {
      return Ok(t);
    }
    self.require_web()?;
    let resp = self
      .ctx
      .http
      .get(format!("{MP}/"))
      .header("accept", "text/html,application/xhtml+xml")
      .send()
      .await?;
    let token = HOME_TOKEN
      .captures(&resp.url)
      .map(|m| m[1].to_owned())
      .or_else(|| page_token(&resp.text()))
      .ok_or_else(expired)?;
    self.set_web_token(&token);
    Ok(token)
  }

  pub fn set_web_token(&self, token: &str) {
    *self.web_token.borrow_mut() = Some(token.to_owned());
  }

  /// A per-session browser fingerprint (32 hex digits) the backend's own
  /// page sends with most calls.
  pub fn fingerprint(&self) -> String {
    if let Some(f) = self.ctx.extra(FINGERPRINT_KEY) {
      return f;
    }
    let f = format!("{:032x}", rand::random::<u128>());
    self.ctx.set_extra(FINGERPRINT_KEY, &f);
    f
  }

  /// GET a JSON endpoint (`f=json&ajax=1` and the token are added).
  pub async fn web_get(&self, path: &str, query: Params) -> Result<Value> {
    self.web_json(path, query, None, true).await
  }

  /// POST a form to a JSON endpoint (token, `f=json`, `ajax=1` added to both).
  pub async fn web_post(&self, path: &str, query: Params, form: Params) -> Result<Value> {
    self.web_json(path, query, Some(form), true).await
  }

  /// Like [`Client::web_post`], but other `ret` codes than an expired
  /// session come back for the caller to read (multi-step flows).
  pub async fn web_post_raw(&self, path: &str, query: Params, form: Params) -> Result<Value> {
    self.web_json(path, query, Some(form), false).await
  }

  async fn web_json(
    &self,
    path: &str,
    query: Params,
    form: Option<Params>,
    checked: bool,
  ) -> Result<Value> {
    for attempt in 0..2 {
      let token = self.web_token().await?;
      let mut q = query.clone();
      q.extend(standard(&token));
      let req = match &form {
        None => self.ctx.http.get(format!("{MP}{path}")),
        Some(f) => {
          let mut f = f.clone();
          f.extend(standard(&token));
          f.push(("fingerprint", self.fingerprint()));
          self
            .ctx
            .http
            .post(format!("{MP}{path}"))
            .header("origin", MP)
            .form(f)
        }
      };
      let v = req
        .queries(q)
        .header(
          "referer",
          format!("{MP}/cgi-bin/home?t=home/index&lang=zh_CN&token={token}"),
        )
        .header("x-requested-with", "XMLHttpRequest")
        .header("accept", "application/json, text/javascript, */*; q=0.01")
        .value()
        .await?;
      match ret(&v) {
        // A token from an older session: look it up again once.
        200040 if attempt == 0 => *self.web_token.borrow_mut() = None,
        200003 | 200040 => return Err(expired()),
        _ if checked => return check(v),
        _ => return Ok(v),
      }
    }
    unreachable!("the second attempt always returns")
  }

  /// An HTML page of the backend (the token is added).
  pub async fn web_page(&self, path: &str, query: Params) -> Result<String> {
    let token = self.web_token().await?;
    let resp = self
      .ctx
      .http
      .get(format!("{MP}{path}"))
      .queries(query)
      .query("token", &token)
      .query("lang", "zh_CN")
      .header("accept", "text/html,application/xhtml+xml")
      .header(
        "referer",
        format!("{MP}/cgi-bin/home?t=home/index&lang=zh_CN&token={token}"),
      )
      .send()
      .await?;
    if resp.url.contains("/cgi-bin/bizlogin") || resp.url.contains("/cgi-bin/loginpage") {
      return Err(expired());
    }
    if resp.header("logicret") == Some("200003") {
      return Err(expired());
    }
    Ok(resp.text())
  }
}

fn standard(token: &str) -> Params {
  vec![
    ("token", token.to_owned()),
    ("lang", "zh_CN".into()),
    ("f", "json".into()),
    ("ajax", "1".into()),
  ]
}

fn expired() -> Error {
  Error::auth("the mp.weixin.qq.com session has expired (backend sessions last a few days)")
    .with_hint(LOGIN_HINT)
}

fn ret(v: &Value) -> i64 {
  v.i64("base_resp.ret").or_else(|| v.i64("ret")).unwrap_or(0)
}

/// `base_resp.ret` 0 passes; the session, token and rate-limit codes map to
/// errors people can act on.
pub fn check(v: Value) -> Result<Value> {
  let code = ret(&v);
  if code == 0 {
    return Ok(v);
  }
  let msg = v
    .str("base_resp.err_msg")
    .or_else(|| v.str("err_msg"))
    .or_else(|| v.str("msg"))
    .unwrap_or_default();
  Err(match code {
    200003 | 200040 => expired(),
    200013 | 15913 => Error::new(
      ErrorCode::RateLimited,
      format!("the backend is rate limiting this account (ret {code} {msg})"),
    )
    .with_hint("stop for an hour or more; long article lists trip it after ~600 articles"),
    200002 => Error::input(format!(
      "the backend refused the arguments (ret 200002 {msg})"
    ))
    .with_hint("for another account's articles: the account may have been renamed or deleted"),
    _ => Error::upstream(format!("backend error ret {code} {msg}")),
  })
}

/// A field that may hold JSON encoded as a string (`publish_page`,
/// `publish_info`, `app_msg_info`, `comment_list` ...), decoded.
pub fn nested(v: &Value, key: &str) -> Value {
  match v.at(key) {
    Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
    other => other.clone(),
  }
}

/// `t: "123"` in the page data, for pages reached without a token redirect.
fn page_token(html: &str) -> Option<String> {
  static T: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bt\s*:\s*"(\d{4,})""#).expect("valid"));
  T.captures(html).map(|m| m[1].to_owned())
}
