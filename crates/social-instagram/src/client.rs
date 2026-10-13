//! Instagram's web API: `https://www.instagram.com/api/v1/…` (and a few
//! `/web/…` writes), called the way the web app calls them — its app id in
//! `X-IG-App-ID`, the session's `csrftoken` echoed in `X-CSRFToken`.
//! Profiles and single posts answer anonymously; nearly everything else
//! wants the browser session (`sessionid`).
//!
//! Instagram blocks fast clients quickly ("Please wait a few minutes",
//! checkpoints): requests are spaced out and those answers map to clear
//! errors instead of retries.

use social_core::http::{Req, Resp};
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};
use social_meta::graph;

pub const WEB: &str = "https://www.instagram.com";
const API: &str = "https://www.instagram.com/api/v1";
/// The web app's id, which the API wants on every call.
const APP_ID: &str = "936619743392459";
pub const SESSION: &str = "sessionid";
const TOKEN_VARS: [&str; 2] = ["INSTAGRAM_ACCESS_TOKEN", "META_ACCESS_TOKEN"];

pub struct Client {
  pub ctx: Ctx,
}

impl Client {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  pub fn has_session(&self) -> bool {
    self.ctx.http.has_cookie(SESSION)
  }

  pub fn require_session(&self, what: &str) -> Result<()> {
    if self.has_session() {
      return Ok(());
    }
    Err(
      Error::auth(format!("{what} needs a logged-in Instagram session"))
        .with_hint("run `social instagram login --browser` (or `--cookie 'sessionid=…; csrftoken=…; ds_user_id=…'`)"),
    )
  }

  /// The logged-in account's numeric id (`ds_user_id`).
  pub fn my_id(&self) -> Option<String> {
    self.ctx.http.cookie("ds_user_id")
  }

  pub fn token(&self) -> Option<String> {
    graph::token(&self.ctx, &TOKEN_VARS)
  }

  fn headers<'a>(&self, req: Req<'a>) -> Req<'a> {
    let mut req = req
      .header("x-ig-app-id", APP_ID)
      .header("x-asbd-id", "359341")
      .header("x-requested-with", "XMLHttpRequest")
      .header("x-ig-www-claim", "0")
      .header("origin", WEB)
      .header("referer", format!("{WEB}/"))
      .header("sec-fetch-site", "same-origin")
      .header("sec-fetch-mode", "cors")
      .header("sec-fetch-dest", "empty");
    if let Some(csrf) = self.ctx.http.cookie("csrftoken") {
      req = req.header("x-csrftoken", csrf);
    }
    req
  }

  /// GET `/api/v1/<path>`.
  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let req = self
      .ctx
      .http
      .get(format!("{API}/{}", path.trim_start_matches('/')))
      .queries(query.iter().map(|(k, v)| (*k, v)));
    check(self.headers(req).send().await?)
  }

  /// POST a form to `/api/v1/<path>`, or to `<path>` itself when it starts
  /// with `/web/` (the web app's own write endpoints).
  pub async fn post(&self, path: &str, form: &[(&str, String)]) -> Result<Value> {
    let url = if path.starts_with("/web/") {
      format!("{WEB}{path}")
    } else {
      format!("{API}/{}", path.trim_start_matches('/'))
    };
    let req = self
      .ctx
      .http
      .post(url)
      .form(form.iter().map(|(k, v)| (*k, v.as_str())));
    check(self.headers(req).send().await?)
  }

  /// The public GraphQL endpoint with a persisted query.
  pub async fn graphql(&self, doc_id: &str, variables: &Value) -> Result<Value> {
    let req = self.ctx.http.post(format!("{WEB}/graphql/query")).form([
      ("doc_id", doc_id.to_owned()),
      ("variables", variables.to_string()),
    ]);
    check(self.headers(req).send().await?)
  }
}

/// Instagram's answers as errors: login walls, checkpoints, "please wait".
fn check(resp: Resp) -> Result<Value> {
  let text = resp.text();
  let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
  let msg = v
    .str("message")
    .or_else(|| v.str("feedback_message"))
    .unwrap_or_default();
  let status = resp.status.as_u16();
  if v.bool("require_login") == Some(true) {
    return Err(
      Error::auth(
        "Instagram answers this only to logged-in sessions (from this network, right now)",
      )
      .with_hint("run `social instagram login --browser`"),
    );
  }
  if resp.url.contains("/accounts/login") || msg == "login_required" || status == 401 {
    return Err(
      Error::auth("Instagram wants a logged-in session for this")
        .with_hint("run `social instagram login --browser`"),
    );
  }
  if msg.contains("checkpoint") || msg.contains("challenge") || v.str("checkpoint_url").is_some() {
    return Err(
      Error::new(
        ErrorCode::VerificationRequired,
        "Instagram asks to confirm this account",
      )
      .with_hint("open instagram.com in the browser, pass the check, then try again later"),
    );
  }
  if status == 429 || msg.contains("Please wait") || v.bool("spam") == Some(true) {
    return Err(
      Error::new(
        ErrorCode::RateLimited,
        "Instagram says to wait a few minutes",
      )
      .with_hint("slow down (--interval 10) and retry later"),
    );
  }
  if status == 404 {
    return Err(Error::not_found(if msg.is_empty() {
      "not found on Instagram".to_owned()
    } else {
      msg
    }));
  }
  if !resp.status.is_success() || v.str("status").as_deref() == Some("fail") {
    let what = if msg.is_empty() {
      social_core::text::truncate(&text, 200)
    } else {
      msg
    };
    return Err(match status {
      403 => Error::new(
        ErrorCode::PermissionDenied,
        format!("Instagram refused: {what}"),
      ),
      400 => Error::input(format!("Instagram: {what}")),
      _ => Error::upstream(format!("Instagram ({status}): {what}")),
    });
  }
  if v.is_null() {
    return Err(Error::upstream(format!(
      "unexpected answer from Instagram: {}",
      social_core::text::truncate(&text, 200)
    )));
  }
  Ok(v)
}
