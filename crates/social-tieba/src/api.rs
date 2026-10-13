//! Tieba transport, over two hosts:
//!
//! - the **client API** (`https://tiebac.baidu.com/c/...`): form POSTs shaped
//!   like the Android app's, signed with `sign` = upper-case MD5 of the sorted
//!   `key=value` pairs followed by `tiebaclient!!!` (TiebaLite's
//!   `SortAndSignInterceptor`). Answers are JSON with an `error_code`. A
//!   logged-in call carries the `BDUSS` cookie as a parameter.
//! - the **mobile web** (`https://tieba.baidu.com/mo/q/...`): GETs with the
//!   session cookies, JSON with `no` / `errno`.
//!
//! The logged-in account (uid, name, `tbs` anti-CSRF token) comes from
//! the mobile web's `newmoindex` and is cached for the run.

use std::cell::RefCell;
use std::collections::HashMap;

use md5::{Digest, Md5};
use rand::Rng;
use social_core::http::Part;
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

pub const WEB: &str = "https://tieba.baidu.com";
const CLIENT: &str = "https://tiebac.baidu.com";
/// Client version of most calls; answers JSON when no protobuf `cmd` is asked.
pub const VERSION: &str = "12.52.1.0";
/// Older version some endpoints still need for JSON (user posts, follow lists).
pub const LEGACY: &str = "8.2.2";
/// Version the app's writes (follow, sign, post) were captured with.
pub const WRITE: &str = "11.10.8.6";
const SECRET: &str = "tiebaclient!!!";
const WEB_UA: &str = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like \
   Gecko) Version/4.0 Chrome/135.0.0.0 Mobile Safari/537.36 tieba/12.52.1.0";
/// Cookie of a logged-in Baidu session.
pub const SESSION_COOKIE: &str = "BDUSS";

/// Parameters of one call (the common ones are added by [`Client`]).
pub type Params = Vec<(&'static str, String)>;

/// The logged-in account.
#[derive(Debug, Clone)]
pub struct Me {
  pub uid: String,
  pub name: String,
  pub tbs: String,
}

pub struct Client {
  pub ctx: Ctx,
  cuid: String,
  client_id: String,
  me: RefCell<Option<Me>>,
  /// User arguments already resolved to a uid in this run.
  uids: RefCell<HashMap<String, String>>,
}

impl Client {
  pub fn new(ctx: Ctx) -> Self {
    let cuid = ctx.extra("cuid").unwrap_or_else(|| {
      let cuid = new_cuid();
      ctx.set_extra("cuid", &cuid);
      cuid
    });
    let now = jiff::Timestamp::now().as_millisecond();
    let client_id = format!("wappc_{now}_{}", rand::rng().random_range(100..1000));
    Self {
      ctx,
      cuid,
      client_id,
      me: RefCell::new(None),
      uids: RefCell::new(HashMap::new()),
    }
  }

  pub fn bduss(&self) -> Option<String> {
    self.ctx.http.cookie(SESSION_COOKIE)
  }

  pub fn stoken(&self) -> String {
    self.ctx.http.cookie("STOKEN").unwrap_or_default()
  }

  pub fn require_login(&self) -> Result<()> {
    self.ctx.require_login(&[SESSION_COOKIE])
  }

  pub fn cached_uid(&self, arg: &str) -> Option<String> {
    self.uids.borrow().get(arg).cloned()
  }

  pub fn cache_uid(&self, arg: &str, uid: &str) {
    self
      .uids
      .borrow_mut()
      .insert(arg.to_owned(), uid.to_owned());
  }

  // ── client API ───────────────────────────────────────────────────────

  /// Signed client call with the default version; fails on a non-zero `error_code`.
  pub async fn call(&self, path: &str, params: Params) -> Result<Value> {
    self.call_as(path, params, VERSION).await
  }

  pub async fn call_as(&self, path: &str, params: Params, version: &str) -> Result<Value> {
    check(self.call_raw(path, params, version).await?)
  }

  /// Signed client call; the answer is returned whatever its `error_code`.
  pub async fn call_raw(&self, path: &str, params: Params, version: &str) -> Result<Value> {
    let form = self.signed(params, version);
    let resp = self
      .client_req(path, version)
      .form(form.iter().map(|(k, v)| (k.as_str(), v.as_str())))
      .send()
      .await?;
    resp.check()?.value()
  }

  /// Signed multipart call (picture uploads): the text parts are signed, the
  /// file part goes last.
  pub async fn call_multipart(
    &self,
    path: &str,
    params: Params,
    file: Part,
    version: &str,
  ) -> Result<Value> {
    let mut parts: Vec<Part> = self
      .signed(params, version)
      .into_iter()
      .map(|(k, v)| Part::text(&k, v))
      .collect();
    parts.push(file);
    let resp = self
      .client_req(path, version)
      .multipart(parts)
      .send()
      .await?;
    check(resp.check()?.value()?)
  }

  fn client_req(&self, path: &str, version: &str) -> social_core::http::Req<'_> {
    self
      .ctx
      .http
      .post(format!("{CLIENT}{path}"))
      // The app sends its own cookie header; the session goes in `BDUSS`.
      .no_cookies()
      .header("cookie", "ka=open")
      .header("user-agent", format!("bdtb for Android {version}"))
      .header("cuid", &self.cuid)
      .header("client_type", "2")
  }

  /// Common parameters, the call's own (which win), sorted and signed.
  fn signed(&self, params: Params, version: &str) -> Vec<(String, String)> {
    let now = jiff::Timestamp::now().as_millisecond().to_string();
    let own: Vec<&str> = params.iter().map(|(k, _)| *k).collect();
    let mut all: Vec<(String, String)> = [
      ("_client_type", "2".to_owned()),
      ("_client_version", version.to_owned()),
      ("_client_id", self.client_id.clone()),
      ("_phone_imei", "000000000000000".to_owned()),
      ("cuid", self.cuid.clone()),
      ("from", "tieba".to_owned()),
      ("model", "SM-G988N".to_owned()),
      ("net_type", "1".to_owned()),
      ("timestamp", now),
    ]
    .into_iter()
    .chain(self.bduss().map(|b| ("BDUSS", b)))
    .filter(|(k, _)| !own.contains(k))
    .map(|(k, v)| (k.to_owned(), v))
    .chain(params.iter().map(|(k, v)| ((*k).to_owned(), v.clone())))
    .collect();
    all.sort_by_cached_key(|(k, v)| format!("{k}={v}"));
    let sign = sign(&all);
    all.push(("sign".to_owned(), sign));
    all
  }

  // ── mobile web ───────────────────────────────────────────────────────

  /// GET a mobile web endpoint; fails on a non-zero `no` / `errno`.
  pub async fn web(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let resp = self
      .ctx
      .http
      .get(format!("{WEB}{path}"))
      .queries(query.iter().map(|(k, v)| (*k, v)))
      .header("user-agent", WEB_UA)
      .header("referer", format!("{WEB}/"))
      .send()
      .await?;
    let v = resp.check()?.value()?;
    let code = v.first_str(&["no", "errno"]).unwrap_or_else(|| "0".into());
    if code == "0" {
      return Ok(v);
    }
    let msg = v.first_str(&["error", "errmsg"]).unwrap_or_default();
    Err(self.error(&code, &msg))
  }

  // ── the account ──────────────────────────────────────────────────────

  /// The logged-in account (uid, tbs).
  pub async fn me(&self) -> Result<Me> {
    if let Some(me) = self.me.borrow().clone() {
      return Ok(me);
    }
    self.require_login()?;
    let v = self
      .web("/mo/q/newmoindex", &[("need_user", "1".into())])
      .await?;
    let d = v.at("data");
    let uid = d.str("uid").filter(|u| u != "0");
    let (Some(uid), true) = (uid, d.bool("is_login") != Some(false)) else {
      return Err(
        Error::auth("Tieba did not accept the session (BDUSS)").with_hint(self.ctx.login_hint()),
      );
    };
    let me = Me {
      uid,
      name: d.first_str(&["name_show", "name"]).unwrap_or_default(),
      tbs: d.first_str(&["tbs", "itb_tbs"]).unwrap_or_default(),
    };
    *self.me.borrow_mut() = Some(me.clone());
    Ok(me)
  }

  /// The anti-CSRF token writes need.
  pub async fn tbs(&self) -> Result<String> {
    Ok(self.me().await?.tbs)
  }

  pub fn error(&self, code: &str, msg: &str) -> Error {
    let e = error(code, msg);
    if e.code == ErrorCode::NotAuthenticated {
      e.with_hint(self.ctx.login_hint())
    } else {
      e
    }
  }
}

/// Upper-case MD5 of the pairs (already sorted) and the app secret.
fn sign(pairs: &[(String, String)]) -> String {
  let mut h = Md5::new();
  for (k, v) in pairs {
    h.update(k.as_bytes());
    h.update(b"=");
    h.update(v.as_bytes());
  }
  h.update(SECRET.as_bytes());
  h.finalize().iter().map(|b| format!("{b:02X}")).collect()
}

/// A device id the way the app makes one (`baidutiebaapp` + a UUID).
fn new_cuid() -> String {
  let mut rng = rand::rng();
  let hex: String = (0..32)
    .map(|_| char::from_digit(rng.random_range(0..16), 16).unwrap_or('0'))
    .collect();
  format!(
    "baidutiebaapp{}-{}-{}-{}-{}",
    &hex[..8],
    &hex[8..12],
    &hex[12..16],
    &hex[16..20],
    &hex[20..]
  )
}

/// The client API's `error_code` envelope (number or string, `0` = fine).
pub fn check(v: Value) -> Result<Value> {
  let code = v.str("error_code").unwrap_or_else(|| "0".into());
  if code == "0" {
    return Ok(v);
  }
  let msg = v
    .first_str(&["error_msg", "error.usermsg", "error.errmsg"])
    .unwrap_or_default();
  Err(error(&code, &msg))
}

/// Map an upstream code and message onto an error code by what it says.
pub fn error(code: &str, msg: &str) -> Error {
  let has = |words: &[&str]| words.iter().any(|w| msg.contains(w));
  let kind = if code == "110000" || has(&["未登录", "登录失败", "not logined", "请先登录"])
  {
    ErrorCode::NotAuthenticated
  } else if code == "220034" || has(&["太快", "频繁", "过于频繁"]) {
    ErrorCode::RateLimited
  } else if has(&["验证码", "安全验证", "vcode"]) {
    ErrorCode::VerificationRequired
  } else if has(&["不存在", "未建立", "已被删除", "已删除", "被删除"]) {
    ErrorCode::NotFound
  } else if has(&["权限", "封禁", "屏蔽", "禁言", "无法"]) {
    ErrorCode::PermissionDenied
  } else {
    ErrorCode::UpstreamError
  };
  let text = if msg.is_empty() {
    format!("Tieba error {code}")
  } else {
    format!("Tieba error {code}: {msg}")
  };
  Error::new(kind, text)
}
