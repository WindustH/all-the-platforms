//! Transport for weibo.com's web API (`/ajax/...`) and the m.weibo.cn mobile
//! API, with upstream errors mapped to `atp_core::Error`.
//!
//! A logged-in session is the browser's weibo.com cookies (`SUB`, `SUBP`,
//! `XSRF-TOKEN` ...). Without one, requests carry a visitor cookie from Sina's
//! visitor system (`genvisitor2`), cached apart from the session so it never
//! looks like a login. m.weibo.cn always gets the visitor cookie: its
//! `.weibo.cn` session differs from weibo.com's, and what it is used for
//! (search, topics, anonymous listings) needs no login.

use std::cell::RefCell;
use std::time::Duration;

use atp_core::http::{Method, Req, Resp, status_error};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

pub const HOME: &str = "https://weibo.com/";
pub const WEB: &str = "https://weibo.com";
pub const MOBILE: &str = "https://m.weibo.cn";
const VISITOR: &str = "https://visitor.passport.weibo.cn/visitor/genvisitor2";
/// Cookie of a logged-in weibo.com session.
pub const LOGIN_COOKIE: &str = "SUB";
const VISITOR_KEY: &str = "visitor";
const VISITOR_TTL: Duration = Duration::from_secs(3 * 86400);
/// Session extra with the logged-in account's id.
const UID_KEY: &str = "uid";
const MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";
const LOGIN_WORDS: &[&str] = &["请先登录", "请登录", "登录后", "用户未登录", "未登录"];
const CAPTCHA_WORDS: &[&str] = &["验证码", "安全验证", "身份验证", "geetest", "滑块"];
const BUSY_WORDS: &[&str] = &["频繁", "稍后再试", "操作过快"];

pub struct Api {
  pub ctx: Ctx,
  visitor: RefCell<Option<String>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Site {
  Web,
  Mobile,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      visitor: RefCell::new(None),
    }
  }

  pub fn logged_in(&self) -> bool {
    self.ctx.http.has_cookie(LOGIN_COOKIE)
  }

  pub fn require_login(&self) -> Result<()> {
    self.ctx.require_login(&[LOGIN_COOKIE])
  }

  /// The logged-in account's id, when an earlier `whoami` saved it.
  pub fn own_uid(&self) -> Option<String> {
    self.ctx.extra(UID_KEY)
  }

  pub fn set_own_uid(&self, uid: &str) {
    self.ctx.set_extra(UID_KEY, uid);
  }

  /// Random pause before a write, as people do not click instantly.
  pub async fn write_pause(&self) {
    self
      .ctx
      .http
      .pause(Duration::from_millis(1200), Duration::from_millis(3500))
      .await;
  }

  // ── visitor cookie ───────────────────────────────────────────────────

  async fn visitor_cookie(&self, fresh: bool) -> Result<String> {
    if !fresh {
      if let Some(c) = self.visitor.borrow().clone() {
        return Ok(c);
      }
      if let Some(c) = self
        .ctx
        .store
        .cache_get::<String>(VISITOR_KEY, VISITOR_TTL)
        .filter(|c| !c.is_empty())
      {
        *self.visitor.borrow_mut() = Some(c.clone());
        return Ok(c);
      }
    }
    let resp = self
      .ctx
      .http
      .post(VISITOR)
      .no_cookies()
      .header("referer", "https://passport.weibo.com/")
      .form([
        ("cb", "visitor_callback"),
        ("from", "weibo"),
        ("tid", ""),
        ("return_url", "https://m.weibo.cn/"),
      ])
      .send()
      .await?;
    let text = resp.text();
    let json = text
      .find('(')
      .and_then(|i| text.rfind(')').map(|j| &text[i + 1..j]))
      .and_then(|s| serde_json::from_str::<Value>(s).ok())
      .unwrap_or_default();
    let (Some(sub), Some(subp)) = (json.str("data.sub"), json.str("data.subp")) else {
      return Err(Error::upstream(
        "Sina's visitor system handed out no cookie; try again later or log in",
      ));
    };
    let cookie = format!("SUB={sub}; SUBP={subp}");
    self.ctx.store.cache_put(VISITOR_KEY, &cookie);
    *self.visitor.borrow_mut() = Some(cookie.clone());
    Ok(cookie)
  }

  // ── requests ─────────────────────────────────────────────────────────

  async fn request(&self, site: Site, method: Method, url: &str, fresh: bool) -> Result<Req<'_>> {
    let http = &self.ctx.http;
    let post = method == Method::POST;
    let mut req = http
      .request(method, url)
      .header("x-requested-with", "XMLHttpRequest");
    req = match site {
      Site::Web => req.header("referer", HOME),
      Site::Mobile => req
        .header("referer", "https://m.weibo.cn/")
        .header("user-agent", MOBILE_UA)
        .header("mweibo-pwa", "1"),
    };
    if site == Site::Web && self.logged_in() {
      if post {
        self.ensure_xsrf().await;
        if let Some(token) = http.cookie("XSRF-TOKEN") {
          req = req.header("x-xsrf-token", token);
        }
        req = req.header("origin", "https://weibo.com");
      }
      return Ok(req);
    }
    let visitor = self.visitor_cookie(fresh).await?;
    Ok(req.no_cookies().header("cookie", visitor))
  }

  /// GET a weibo.com endpoint (`/ajax/...`).
  pub async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
    self
      .get_site(Site::Web, &format!("{WEB}{path}"), query)
      .await
  }

  /// GET an m.weibo.cn endpoint (`/api/container/getIndex` ...).
  pub async fn get_mobile(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
    self
      .get_site(Site::Mobile, &format!("{MOBILE}{path}"), query)
      .await
  }

  async fn get_site(&self, site: Site, url: &str, query: &[(&str, &str)]) -> Result<Value> {
    let mut fresh = false;
    loop {
      let resp = self
        .request(site, Method::GET, url, fresh)
        .await?
        .queries(query.iter().copied())
        .send()
        .await?;
      match self.check(resp, site) {
        Err(e) if e.code == Retry::CODE && !fresh && !self.uses_session(site) => {
          tracing::debug!("visitor cookie refused; asking for a new one");
          fresh = true;
        }
        other => return other.map_err(Retry::finish),
      }
    }
  }

  fn uses_session(&self, site: Site) -> bool {
    site == Site::Web && self.logged_in()
  }

  /// POST a form to weibo.com as the logged-in account.
  pub async fn post_form(&self, path: &str, form: &[(&str, &str)]) -> Result<Value> {
    self.require_login()?;
    let resp = self
      .request(Site::Web, Method::POST, &format!("{WEB}{path}"), false)
      .await?
      .form(form.iter().copied())
      .send()
      .await?;
    self.check(resp, Site::Web).map_err(Retry::finish)
  }

  /// POST a JSON body to weibo.com as the logged-in account.
  pub async fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
    self.require_login()?;
    let resp = self
      .request(Site::Web, Method::POST, &format!("{WEB}{path}"), false)
      .await?
      .json(body)
      .send()
      .await?;
    self.check(resp, Site::Web).map_err(Retry::finish)
  }

  /// weibo.com sets `XSRF-TOKEN` on its pages; writes echo it in a header.
  async fn ensure_xsrf(&self) {
    let http = &self.ctx.http;
    if http.has_cookie("XSRF-TOKEN") {
      return;
    }
    let _ = http
      .get(format!("{WEB}/ajax/config/get_config"))
      .header("referer", HOME)
      .header("x-requested-with", "XMLHttpRequest")
      .send()
      .await;
    if !http.has_cookie("XSRF-TOKEN") {
      let _ = http.get(HOME).header("accept", "text/html").send().await;
    }
  }

  // ── errors ───────────────────────────────────────────────────────────

  fn check(&self, resp: Resp, site: Site) -> Result<Value> {
    let session = self.uses_session(site);
    if !resp.status.is_success() {
      return Err(status_error(resp.status, &resp.text()));
    }
    let text = resp.text();
    let trimmed = text.trim_start();
    if trimmed.starts_with('<') {
      if trimmed.contains("Sina Visitor System") || resp.url.contains("passport.weibo") {
        return Err(if session {
          expired(self)
        } else {
          Retry::error("the visitor page came back")
        });
      }
      return Err(Error::upstream(
        "Weibo answered with a web page instead of data",
      ));
    }
    let v: Value = serde_json::from_str(trimmed).map_err(|e| {
      Error::upstream(format!(
        "unexpected Weibo response ({e}): {}",
        atp_core::text::truncate(trimmed, 160)
      ))
    })?;
    if v.as_object().is_some_and(|o| o.is_empty()) {
      return Err(if session {
        Error::upstream("Weibo returned nothing for this")
      } else {
        needs_login(self, "this list")
      });
    }
    let ok = v.i64("ok");
    let message = v
      .first_str(&["msg", "message", "error", "error_msg", "empty_tip"])
      .unwrap_or_default();
    if ok == Some(-100) {
      return Err(if session {
        expired(self)
      } else {
        Retry::error("login wall")
      });
    }
    // m.weibo.cn answers an empty (or hidden) list with `ok: 0`.
    if ok == Some(0) && site == Site::Mobile && message.contains("没有内容") {
      return Ok(atp_core::json!({"ok": 1, "data": {"cards": []}}));
    }
    if ok == Some(0) || v.str("error_code").is_some() {
      return Err(classify(self, &message, session));
    }
    // Lists answer `ok: 1` with an empty list and a login tip.
    if !session && LOGIN_WORDS.iter().any(|w| message.contains(w)) {
      return Err(needs_login(self, "this"));
    }
    Ok(v)
  }
}

fn classify(api: &Api, message: &str, session: bool) -> Error {
  let message = if message.is_empty() {
    "Weibo refused the request".to_owned()
  } else {
    message.to_owned()
  };
  if LOGIN_WORDS.iter().any(|w| message.contains(w)) {
    return if session {
      expired(api)
    } else {
      needs_login(api, "this")
    };
  }
  if CAPTCHA_WORDS.iter().any(|w| message.contains(w)) {
    return Error::new(ErrorCode::VerificationRequired, message)
      .with_hint("open weibo.com in the browser, pass the check, and retry later");
  }
  if BUSY_WORDS.iter().any(|w| message.contains(w)) {
    return Error::new(ErrorCode::RateLimited, message).with_hint("slow down and retry later");
  }
  if message.contains("不存在") || message.contains("已删除") || message.contains("已被删除")
  {
    return Error::not_found(message);
  }
  if message.contains("权限") || message.contains("不可见") || message.contains("隐私") {
    return Error::new(ErrorCode::PermissionDenied, message);
  }
  Error::upstream(message)
}

fn needs_login(api: &Api, what: &str) -> Error {
  Error::auth(format!("Weibo shows {what} to logged-in accounts only"))
    .with_hint(api.ctx.login_hint())
}

fn expired(api: &Api) -> Error {
  Error::auth("the Weibo session has expired or was refused").with_hint(api.ctx.login_hint())
}

/// A visitor request that may succeed with a new visitor cookie.
struct Retry;

impl Retry {
  const CODE: ErrorCode = ErrorCode::SignatureError;

  fn error(why: &str) -> Error {
    Error::new(Self::CODE, why)
  }

  /// A retry that did not help: Weibo wants a login.
  fn finish(error: Error) -> Error {
    if error.code == Self::CODE {
      Error::auth("Weibo shows this to logged-in accounts only")
        .with_hint("log in with `atp weibo login` (QR code), `--browser` or `--cookie '...'`")
    } else {
      error
    }
  }
}
