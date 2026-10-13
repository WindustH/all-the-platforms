//! Transport. Two planes, as the web client has them:
//!
//! - **Pages** (`www.tiktok.com/@user`, `/@user/video/<id>` ...) carry their
//!   data in the `__UNIVERSAL_DATA_FOR_REHYDRATION__` script and need no
//!   signature. A SlardarWAF proof-of-work page in front of them is solved
//!   ([`crate::waf`]) and the page loaded again.
//! - **API** (`www.tiktok.com/api/...`) takes the browser's device block, an
//!   `msToken` and `X-Bogus` ([`crate::sign`]). TikTok scores these calls; a
//!   refused one comes back as an empty 200, mapped to `verification_required`.
//!
//! The device block (web id, region) comes from the first page loaded, the
//! `app-context` of its data. Writes go out as POST with the `tt-csrf-token`.

use std::cell::RefCell;
use std::time::Duration;

use atp_core::http::{Method, Req, USER_AGENT};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

use crate::{sign, waf};

pub const WWW: &str = "https://www.tiktok.com";
/// Cookie of a logged-in session.
pub const SESSION_COOKIE: &str = "sessionid";
const SCRIPT_ID: &str = "__UNIVERSAL_DATA_FOR_REHYDRATION__";
const WAF_COOKIES: &[&str] = &["_wafchallengeid", "waforiginalreid", "waforigin_id"];
const WALLED_HINT: &str = "TikTok answers its API only to sessions it trusts: log in \
   (`atp tiktok login --browser`), or retry later / from another network (--proxy)";

/// What the web app knows about this visitor: from a page's `webapp.app-context`.
#[derive(Debug, Clone, Default)]
pub struct App {
  pub web_id: String,
  pub web_id_time: String,
  pub region: String,
  pub language: String,
  /// The logged-in account (`user` of the app context), if any.
  pub user: Option<Value>,
}

pub struct Api {
  pub ctx: Ctx,
  app: RefCell<Option<App>>,
}

impl Api {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("accept-language", "en-US,en;q=0.9");
    Self {
      ctx,
      app: RefCell::new(None),
    }
  }

  pub fn logged_in(&self) -> bool {
    self.ctx.http.has_cookie(SESSION_COOKIE)
  }

  pub fn require_login(&self) -> Result<()> {
    self.ctx.require_login(&[SESSION_COOKIE])
  }

  /// Random pause before a write, as people do.
  pub async fn write_pause(&self) {
    self
      .ctx
      .http
      .pause(Duration::from_millis(1500), Duration::from_millis(4000))
      .await;
  }

  // ── pages ────────────────────────────────────────────────────────────

  /// The `__DEFAULT_SCOPE__` of a page's rehydration data.
  pub async fn page(&self, url: &str) -> Result<Value> {
    let html = self.html(url).await?;
    let scope = universal(&html)
      .ok_or_else(|| Error::upstream("the page carried no data (TikTok changed its layout?)"))?;
    let context = node(&scope, "webapp.app-context");
    if !context.is_null() {
      self.remember(context);
    }
    Ok(scope)
  }

  /// A page's HTML, through the WAF challenge when there is one.
  pub async fn html(&self, url: &str) -> Result<String> {
    for attempt in 0..3 {
      let resp = self
        .ctx
        .http
        .get(url)
        .header(
          "accept",
          "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header("referer", format!("{WWW}/"))
        .send()
        .await?;
      if resp.status.as_u16() == 429 {
        return Err(Error::new(
          ErrorCode::RateLimited,
          "TikTok is rate limiting; wait a while",
        ));
      }
      let html = resp.text();
      if !waf::is_challenge(&html) {
        self.clear_waf();
        if !resp.status.is_success() && !html.contains(SCRIPT_ID) {
          return Err(atp_core::http::status_error(resp.status, &html));
        }
        return Ok(html);
      }
      let Some(cookies) = waf::solve(&html).filter(|_| attempt < 2) else {
        break;
      };
      tracing::debug!("solved the WAF challenge of {url}");
      for (name, value) in cookies {
        self.ctx.http.set_cookie(&name, &value);
      }
    }
    self.clear_waf();
    Err(
      Error::new(
        ErrorCode::VerificationRequired,
        "TikTok keeps serving its bot check",
      )
      .with_hint("open tiktok.com in a browser, or retry later / through --proxy"),
    )
  }

  fn clear_waf(&self) {
    for name in WAF_COOKIES {
      self.ctx.http.remove_cookie(name);
    }
  }

  fn remember(&self, context: &Value) {
    let app = App {
      web_id: context.str("wid").unwrap_or_default(),
      web_id_time: context.str("webIdCreatedTime").unwrap_or_default(),
      region: context.str("region").unwrap_or_else(|| "US".into()),
      language: context.str("language").unwrap_or_else(|| "en".into()),
      user: Some(context.at("user").clone()).filter(|u| u.str("uid").is_some()),
    };
    *self.app.borrow_mut() = Some(app);
  }

  /// The visitor's app context, loading the explore page once if no page was read yet.
  pub async fn app(&self) -> Result<App> {
    if let Some(app) = self.app.borrow().clone() {
      return Ok(app);
    }
    self.page(&format!("{WWW}/explore")).await?;
    Ok(self.app.borrow().clone().unwrap_or_default())
  }

  // ── API ──────────────────────────────────────────────────────────────

  /// GET a signed `/api/...` endpoint.
  pub async fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
    self.call(Method::GET, path, params).await
  }

  /// POST a signed `/api/...` endpoint (writes; parameters in the query).
  pub async fn post(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
    self.require_login()?;
    self.call(Method::POST, path, params).await
  }

  async fn call(&self, method: Method, path: &str, params: &[(&str, String)]) -> Result<Value> {
    let query = self.query(params).await?;
    let now = jiff::Timestamp::now().as_second() as u32;
    let url = format!(
      "{WWW}{path}?{query}&X-Bogus={}",
      sign::x_bogus(&query, USER_AGENT, now)
    );
    let mut req: Req<'_> = self
      .ctx
      .http
      .request(method.clone(), url)
      .header("referer", format!("{WWW}/"))
      .header("sec-fetch-site", "same-origin")
      .header("sec-fetch-mode", "cors")
      .header("sec-fetch-dest", "empty");
    if method == Method::POST {
      req = req
        .header("origin", WWW)
        .header(
          "tt-csrf-token",
          self.ctx.http.cookie("tt_csrf_token").unwrap_or_default(),
        )
        .bytes(Vec::new(), "application/x-www-form-urlencoded");
    }
    let resp = req.send().await?;
    match resp.status.as_u16() {
      429 => {
        return Err(Error::new(
          ErrorCode::RateLimited,
          "TikTok is rate limiting; wait a while",
        ));
      }
      401 => {
        return Err(Error::auth("TikTok wants a login for this").with_hint(self.ctx.login_hint()));
      }
      _ => {}
    }
    if resp.body.iter().all(u8::is_ascii_whitespace) {
      return Err(
        Error::new(
          ErrorCode::VerificationRequired,
          format!("TikTok refused the request to {path} (empty answer)"),
        )
        .with_hint(WALLED_HINT),
      );
    }
    let v = resp.check()?.value()?;
    check(path, v)
  }

  /// The browser's device block, the parameters, then the `msToken`,
  /// percent-encoded the way the web client sends them.
  async fn query(&self, params: &[(&str, String)]) -> Result<String> {
    let app = self.app().await?;
    let browser_version = USER_AGENT.trim_start_matches("Mozilla/");
    let mut all: Vec<(&str, String)> = vec![
      ("WebIdLastTime", app.web_id_time.clone()),
      ("aid", "1988".into()),
      ("app_language", app.language.clone()),
      ("app_name", "tiktok_web".into()),
      ("browser_language", "en-US".into()),
      ("browser_name", "Mozilla".into()),
      ("browser_online", "true".into()),
      ("browser_platform", "MacIntel".into()),
      ("browser_version", browser_version.into()),
      ("channel", "tiktok_web".into()),
      ("cookie_enabled", "true".into()),
      ("data_collection_enabled", "true".into()),
      ("device_id", app.web_id.clone()),
      ("device_platform", "web_pc".into()),
      ("focus_state", "true".into()),
      ("history_len", "3".into()),
      ("is_fullscreen", "false".into()),
      ("is_page_visible", "true".into()),
      ("language", app.language.clone()),
      ("odinId", odin_id(&app)),
      ("os", "mac".into()),
      ("priority_region", app.region.clone()),
      ("referer", String::new()),
      ("region", app.region.clone()),
      ("screen_height", "1080".into()),
      ("screen_width", "1920".into()),
      ("tz_name", "America/New_York".into()),
      ("user_is_login", app.user.is_some().to_string()),
      ("webcast_language", app.language.clone()),
    ];
    for (k, v) in params {
      all.retain(|(key, _)| key != k);
      all.push((k, v.clone()));
    }
    let token = self
      .ctx
      .http
      .cookie("msToken")
      .unwrap_or_else(sign::ms_token);
    all.push(("msToken", token));
    Ok(
      all
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&"),
    )
  }
}

fn odin_id(app: &App) -> String {
  app
    .user
    .as_ref()
    .and_then(|u| u.str("uid"))
    .unwrap_or_else(|| "0".into())
}

/// `encodeURIComponent`.
pub fn encode(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for b in s.bytes() {
    match b {
      b'A'..=b'Z'
      | b'a'..=b'z'
      | b'0'..=b'9'
      | b'-'
      | b'_'
      | b'.'
      | b'!'
      | b'~'
      | b'*'
      | b'\''
      | b'('
      | b')' => out.push(b as char),
      _ => out.push_str(&format!("%{b:02X}")),
    }
  }
  out
}

/// A node of page data by its key (keys contain dots, so no `ValueExt::at`).
pub fn node<'a>(scope: &'a Value, key: &str) -> &'a Value {
  static NULL: Value = Value::Null;
  scope.get(key).unwrap_or(&NULL)
}

/// The rehydration data of a page, `__DEFAULT_SCOPE__` of it.
pub fn universal(html: &str) -> Option<Value> {
  let at = html.find(&format!("id=\"{SCRIPT_ID}\""))?;
  let start = at + html[at..].find('>')? + 1;
  let end = start + html[start..].find("</script>")?;
  let v: Value = serde_json::from_str(html[start..end].trim()).ok()?;
  Some(v.at("__DEFAULT_SCOPE__").clone())
}

/// `statusCode` / `status_code` of an API answer, mapped to errors.
fn check(path: &str, v: Value) -> Result<Value> {
  let code = v
    .i64("statusCode")
    .or_else(|| v.i64("status_code"))
    .unwrap_or(0);
  if code == 0 {
    return Ok(v);
  }
  let msg = v
    .first_str(&["statusMsg", "status_msg", "message"])
    .unwrap_or_default();
  Err(status_error(path, code, &msg))
}

/// The status codes TikTok answers with, as errors.
pub fn status_error(what: &str, code: i64, msg: &str) -> Error {
  let text = if msg.is_empty() {
    format!("TikTok answered {code} to {what}")
  } else {
    format!("TikTok answered {code} to {what}: {msg}")
  };
  let lower = msg.to_ascii_lowercase();
  match code {
    8 | 2053 | 10002 => Error::auth(text).with_hint("log in: `atp tiktok login --browser`"),
    10201 | 10202 | 10204 | 10221 | 10223 | 10225 => Error::not_found(text),
    10216 | 10222 | 10224 => Error::new(ErrorCode::PermissionDenied, text),
    10000 | 10101 => Error::new(ErrorCode::VerificationRequired, text).with_hint(WALLED_HINT),
    _ if lower.contains("login") => Error::auth(text),
    _ => Error::upstream(text),
  }
}
