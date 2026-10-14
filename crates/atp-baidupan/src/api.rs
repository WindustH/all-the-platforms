//! Baidu Netdisk transport: which route a command takes, the web route's
//! calls, and Baidu's error codes for both routes.
//!
//! The web route is what the web drive itself calls on `pan.baidu.com`
//! (and `pcs.baidu.com` for restores) with the session cookies, its app id
//! `250528` and, for changes, the page token `bdstoken`. Answers carry
//! `errno` (0 on success), or `error_code` on the PCS and offline-download
//! hosts. Risk checks (8002, 9019) and the per-minute limit (31034) are
//! tried again after a pause; everything else maps onto atp errors in
//! [`refusal`], with the login it needs in the hint.

use std::cell::{Cell, RefCell};

use atp_core::http::{Method, Part, Req, Resp, status_error};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, task};

use crate::open::Open;

pub const PAN: &str = "https://pan.baidu.com";
/// The web drive's own app id, sent with its calls.
pub const WEB_APP_ID: &str = "250528";
const REFERER: &str = "https://pan.baidu.com/disk/main";
/// Calls a risk check turns away get this many tries in all.
const TRIES: u32 = 3;
/// Codes another try after a pause may get past.
const RETRY: [i64; 3] = [8002, 9019, 31034];

pub const WEB_HINT: &str = "log in to the web drive: run `atp baidupan login` (QR code), \
  `atp baidupan login --browser` or `--cookie 'BDUSS=...; STOKEN=...'`";
pub const OPEN_HINT: &str = "register your own app on Baidu's open platform \
  (https://pan.baidu.com/union, \"个人使用\"), then run \
  `atp baidupan login --token APP_KEY:SECRET_KEY`";
const RISK_HINT: &str = "Baidu wants a check: open https://pan.baidu.com in a browser logged in \
  to this account, pass it, then try again in a few minutes";
const SANDBOX_HINT: &str = "open-platform apps reach only what their user granted: apps created \
  after 2026-06-03 download, upload and change files only under /apps/<app name>, and an app \
  that was not reviewed serves its first 10 users only";

/// The route a command goes over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
  /// The web drive, with the session cookies.
  Web,
  /// The open platform, with the OAuth token of the user's app.
  Open,
}

/// The parts of one web call besides its path.
#[derive(Default)]
pub struct Call<'a> {
  pub query: Vec<(&'a str, String)>,
  pub form: Option<Vec<(&'a str, String)>>,
  /// Send `form` as `multipart/form-data`.
  pub multipart: bool,
  pub referer: Option<String>,
  /// Cookies to send instead of the jar's of the same names (a share's
  /// `BDCLND`); with it, cookies the answer sets are not kept.
  pub cookie: Option<String>,
}

pub struct Api {
  pub ctx: Ctx,
  /// `/api/loginStatus` of this run.
  status: RefCell<Option<Value>>,
  /// `login --token` ran in this run: the new token is what to check.
  pub token_login: Cell<bool>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      status: RefCell::new(None),
      token_login: Cell::new(false),
    }
  }

  /// The open-platform route.
  pub fn open(&self) -> Open<'_> {
    Open::new(&self.ctx)
  }

  pub fn web_login(&self) -> bool {
    self.ctx.http.has_cookie("BDUSS")
  }

  /// The route for something both routes do: the web drive when its login
  /// is there (it spares the app's call quota), else the open platform.
  pub fn route(&self) -> Result<Route> {
    if self.web_login() {
      Ok(Route::Web)
    } else if self.open().logged_in() {
      Ok(Route::Open)
    } else {
      Err(Error::auth("not logged in").with_hint(self.ctx.login_hint()))
    }
  }

  /// Fail unless the web login is there; `what` names the command.
  pub fn require_web(&self, what: &str) -> Result<()> {
    if self.web_login() {
      return Ok(());
    }
    Err(
      Error::auth(format!(
        "{what} goes through the web drive, which needs the web login"
      ))
      .with_hint(WEB_HINT),
    )
  }

  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let call = Call {
      query: query.to_vec(),
      ..Call::default()
    };
    self.call(Method::GET, path, call).await
  }

  pub async fn post(
    &self,
    path: &str,
    query: &[(&str, String)],
    form: &[(&str, String)],
  ) -> Result<Value> {
    let call = Call {
      query: query.to_vec(),
      form: Some(form.to_vec()),
      ..Call::default()
    };
    self.call(Method::POST, path, call).await
  }

  /// One web call (`path` under `pan.baidu.com`, or a whole URL); the whole
  /// answer, or the error it carries.
  pub async fn call(&self, method: Method, path: &str, call: Call<'_>) -> Result<Value> {
    if !self.web_login() {
      return Err(Error::auth("not logged in to the web drive").with_hint(WEB_HINT));
    }
    task::retry(TRIES, retryable, async || {
      let resp = self.request(method.clone(), path, &call).send().await?;
      check(&resp, path, Route::Web)
    })
    .await
  }

  fn request(&self, method: Method, path: &str, call: &Call<'_>) -> Req<'_> {
    let url = if path.starts_with("https://") {
      path.to_owned()
    } else {
      format!("{PAN}{path}")
    };
    let mut req = self
      .ctx
      .http
      .request(method, url)
      .query("clienttype", "0")
      .query("app_id", WEB_APP_ID)
      .query("web", "1")
      .queries(call.query.iter().cloned())
      .header("referer", call.referer.as_deref().unwrap_or(REFERER))
      .header("origin", PAN);
    match &call.form {
      Some(form) if call.multipart => {
        let parts = form.iter().map(|(k, v)| Part::text(k, v.clone()));
        req = req.multipart(parts.collect());
      }
      Some(form) => req = req.form(form.iter().cloned()),
      None => {}
    }
    if let Some(extra) = &call.cookie {
      req = req.no_cookies().header("cookie", self.cookies_with(extra));
    }
    req
  }

  /// The jar's cookies, those named in `extra` replaced by its values.
  fn cookies_with(&self, extra: &str) -> String {
    let pairs: Vec<&str> = extra
      .split(';')
      .map(str::trim)
      .filter(|p| !p.is_empty())
      .collect();
    let named = |name: &str| pairs.iter().any(|p| p.split('=').next() == Some(name));
    let mut cookies: Vec<String> = self
      .ctx
      .http
      .cookies()
      .into_iter()
      .filter(|(name, _)| !named(name))
      .map(|(name, value)| format!("{name}={value}"))
      .collect();
    cookies.extend(pairs.iter().map(|p| (*p).to_owned()));
    cookies.join("; ")
  }

  /// The account behind the cookies (`login_info`), asked once per run.
  pub async fn login_status(&self) -> Result<Value> {
    if let Some(v) = self.status.borrow().clone() {
      return Ok(v);
    }
    let v = self.get("/api/loginStatus", &[]).await?;
    let info = v.at("login_info").clone();
    if info.str("uk").is_none() {
      return Err(Error::auth("Baidu does not accept this web login").with_hint(WEB_HINT));
    }
    *self.status.borrow_mut() = Some(info.clone());
    Ok(info)
  }

  /// The page token changes on the web route carry.
  pub async fn bdstoken(&self) -> Result<String> {
    if let Some(token) = self.login_status().await?.str("bdstoken") {
      return Ok(token);
    }
    let query = [("fields", r#"["bdstoken"]"#.to_owned())];
    let v = self.get("/api/gettemplatevariable", &query).await?;
    v.str("result.bdstoken")
      .ok_or_else(|| Error::upstream("Baidu gave no bdstoken for this session"))
  }
}

/// The answer, or the error it carries; `what` names the call.
pub fn check(resp: &Resp, what: &str, route: Route) -> Result<Value> {
  let Ok(v) = resp.value() else {
    let e = status_error(resp.status, &resp.text());
    return Err(match e.code {
      ErrorCode::NotAuthenticated => e.with_hint(login_hint(route)),
      _ => e,
    });
  };
  let code = errno(&v);
  if code == 0 && resp.status.is_success() {
    return Ok(v);
  }
  if code == 0 {
    return Err(status_error(resp.status, &resp.text()));
  }
  Err(refusal(&v, code, what, route))
}

/// The code of an answer: `errno`, else `error_code`; 0 when neither.
pub fn errno(v: &Value) -> i64 {
  v.i64("errno")
    .or_else(|| v.i64("error_code"))
    .unwrap_or_default()
}

/// The atp error for a refused call.
pub fn refusal(v: &Value, code: i64, what: &str, route: Route) -> Error {
  let text = v
    .first_str(&["show_msg", "errmsg", "error_msg", "err_msg"])
    .filter(|t| !t.trim().is_empty())
    .unwrap_or_else(|| meaning(code).to_owned());
  let message = format!("Baidu {what}: {text} (errno {code})");
  // PCS hosts number their own errors: 110 and 111 are tokens there.
  let pcs = v.get("errno").is_none();
  let new = |kind| Error::new(kind, message.clone());
  match code {
    -6 | 31045 | 20016 | 20017 => Error::auth(message).with_hint(login_hint(route)),
    110 | 111 if pcs => Error::auth(message).with_hint(login_hint(route)),
    8001 | 8002 | 9019 | 132 | -19 | -62 => new(ErrorCode::VerificationRequired)
      .with_url(PAN)
      .with_hint(RISK_HINT),
    31034 => new(ErrorCode::RateLimited)
      .with_hint("too many calls a minute (a recursive listing allows 8 to 10); wait a minute"),
    20012 => new(ErrorCode::RateLimited).with_hint(
      "the app's call quota is used up: an app that was not reviewed gets about 10 calls an hour",
    ),
    20011 | 20013 | 20020 | 31064 | 31024 => {
      new(ErrorCode::PermissionDenied).with_hint(SANDBOX_HINT)
    }
    -10 | 31112 => new(ErrorCode::PermissionDenied).with_hint("the drive is full; free some space"),
    9013 => new(ErrorCode::PermissionDenied)
      .with_hint("Baidu flagged this account; its app's customer service can lift it"),
    31326 | 31362 => new(ErrorCode::PermissionDenied),
    -9 | -3 | 31066 | 2131 | 31360 => new(ErrorCode::NotFound),
    -8 | 31061 => Error::input(message).with_hint("pick another name or remove the other one"),
    12 => transfer(v, message),
    4 => Error::input(message).with_hint("the target folder holds these files already"),
    -7 if route == Route::Open => new(ErrorCode::PermissionDenied).with_hint(SANDBOX_HINT),
    -7 | 2 | 31023 | 31062 => Error::input(message),
    _ => Error::upstream(message),
  }
}

/// Code 12: a save of shared files refused, for a count limit or a name
/// taken in the target folder.
fn transfer(v: &Value, message: String) -> Error {
  if let Some(limit) = v.u64("target_file_nums_limit") {
    return Error::new(ErrorCode::PermissionDenied, message).with_hint(format!(
      "one save takes at most {limit} files for this account (free 500, VIP 3000, SVIP 50000); \
       save the share's folders one by one"
    ));
  }
  if v.i64("info.0.errno") == Some(-30) {
    return Error::input(message)
      .with_hint("the target folder holds entries of the same names; save into another (--to)");
  }
  Error::upstream(message)
}

/// What a code means when the answer does not say.
fn meaning(code: i64) -> &'static str {
  match code {
    -6 => "not logged in",
    -7 => "bad name, or no access",
    -8 => "it exists already",
    -9 => "no such file or folder",
    -10 => "the drive is full",
    2 | 31023 => "bad parameters",
    111 => "another background task is running",
    8001 | 9019 => "a check is needed",
    8002 => "Baidu's risk control turned it away",
    20012 => "call quota used up",
    20020 => "the path is outside what the app may reach",
    31034 => "too many calls",
    31064 => "the path is outside the app's folder",
    31326 => "refused as hotlinking",
    31360 => "the link expired",
    _ => "request failed",
  }
}

fn login_hint(route: Route) -> &'static str {
  match route {
    Route::Web => WEB_HINT,
    Route::Open => OPEN_HINT,
  }
}

/// Risk checks and the per-minute limit, which a later try may pass.
pub fn retryable(e: &Error) -> bool {
  RETRY
    .iter()
    .any(|code| e.message.ends_with(&format!("(errno {code})")))
}
