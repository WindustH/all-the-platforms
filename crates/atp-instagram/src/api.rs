//! Instagram's web API: `https://www.instagram.com/api/v1/…` (and a few
//! `/web/…` writes), called the way the web app calls them — its app id in
//! `X-IG-App-ID`, the session's `csrftoken` echoed in `X-CSRFToken`.
//! Profiles and single posts answer anonymously; nearly everything else
//! wants the browser session (`sessionid`).
//!
//! Instagram blocks fast clients quickly ("Please wait a few minutes",
//! checkpoints): requests are spaced out and those answers map to clear
//! errors instead of retries.

use atp_core::http::{Req, Resp};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};
use atp_meta::graph;

use crate::{parse, refs};

pub const WEB: &str = "https://www.instagram.com";
const API: &str = "https://www.instagram.com/api/v1";
/// The web app's id, which the API wants on every call.
const APP_ID: &str = "936619743392459";
pub const SESSION: &str = "sessionid";
const TOKEN_VARS: [&str; 2] = ["INSTAGRAM_ACCESS_TOKEN", "META_ACCESS_TOKEN"];
/// PolarisPostActionLoadPostQueryQuery: one post by shortcode, anonymously.
const POST_DOC: &str = "8845758582119845";

/// Query parameters or form fields of the web API.
pub type Form = Vec<(&'static str, String)>;

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
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
        .with_hint("run `atp instagram login --browser` (or `--cookie 'sessionid=…; csrftoken=…; ds_user_id=…'`)"),
    )
  }

  /// The logged-in account's numeric id (`ds_user_id`).
  pub fn my_id(&self) -> Option<String> {
    self.ctx.http.cookie("ds_user_id")
  }

  /// Fail unless `user` is unset or the logged-in account.
  pub fn require_own(&self, user: Option<&str>, what: &str) -> Result<()> {
    if user.is_some_and(|u| Some(u.to_owned()) != self.my_id()) {
      return Err(Error::unsupported(&format!("{what} of another account")));
    }
    Ok(())
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

  /// A profile's raw user object, by username (anonymous) or numeric id
  /// (logged in).
  pub async fn profile(&self, arg: &str) -> Result<Value> {
    let Some(name) = refs::username(arg) else {
      self.require_session("looking up a user by id")?;
      let v = self
        .get(&format!("users/{}/info/", arg.trim()), &[])
        .await?;
      return Ok(v.at("user").clone());
    };
    let v = self
      .get("users/web_profile_info/", &[("username", name.clone())])
      .await?;
    let user = v.at("data.user");
    if !user.is_object() {
      return Err(Error::not_found(format!("no Instagram user @{name}")));
    }
    Ok(user.clone())
  }

  /// The numeric id of a user argument.
  pub async fn user_id(&self, arg: &str) -> Result<String> {
    if refs::username(arg).is_none() {
      return Ok(arg.trim().to_owned());
    }
    Ok(parse::user(&self.profile(arg).await?).id)
  }

  /// A post through the public GraphQL query (no session needed).
  pub async fn public_post(&self, arg: &str) -> Result<Value> {
    let code = match refs::shortcode(arg) {
      Ok(code) => code,
      Err(_) => refs::encode(&refs::media_id(arg)?).ok_or_else(|| Error::input("bad post id"))?,
    };
    let variables = json!({
      "shortcode": code,
      "fetch_tagged_user_count": null,
      "hoisted_comment_id": null,
      "hoisted_reply_id": null,
    });
    let v = self.graphql(POST_DOC, &variables).await?;
    let media = v.at("data.xdt_shortcode_media");
    if !media.is_object() {
      return Err(Error::not_found(format!(
        "post {code} is not public (or does not exist)"
      )));
    }
    Ok(media.clone())
  }

  /// The direct inbox: threads with their newest item.
  pub async fn inbox(&self, cursor: Option<&str>, limit: usize) -> Result<Value> {
    let mut query: Form = vec![
      ("persistentBadging", "true".to_owned()),
      ("limit", limit.to_string()),
      ("thread_message_limit", "1".to_owned()),
    ];
    if let Some(cursor) = cursor {
      query.push(("cursor", cursor.to_owned()));
    }
    self.get("direct_v2/inbox/", &query).await
  }
}

/// `query` with `key=cursor` added when a page asks to continue.
pub fn with_cursor(mut query: Form, key: &'static str, cursor: Option<&String>) -> Form {
  if let Some(cursor) = cursor {
    query.push((key, cursor.clone()));
  }
  query
}

/// Instagram's answers as errors: login walls, checkpoints, "please wait".
fn check(resp: Resp) -> Result<Value> {
  let text = resp.text();
  let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
  let message = v
    .str("message")
    .or_else(|| v.str("feedback_message"))
    .unwrap_or_default();
  let status = resp.status.as_u16();
  if let Some(e) = refusal(&resp, &v, &message) {
    return Err(e);
  }
  if status == 404 {
    return Err(Error::not_found(if message.is_empty() {
      "not found on Instagram".to_owned()
    } else {
      message
    }));
  }
  if !resp.status.is_success() || v.str("status").as_deref() == Some("fail") {
    let what = if message.is_empty() {
      atp_core::text::truncate(&text, 200)
    } else {
      message
    };
    return Err(failure(status, &what));
  }
  if v.is_null() {
    return Err(Error::upstream(format!(
      "unexpected answer from Instagram: {}",
      atp_core::text::truncate(&text, 200)
    )));
  }
  Ok(v)
}

/// Login walls, checkpoints and "please wait".
fn refusal(resp: &Resp, v: &Value, message: &str) -> Option<Error> {
  let status = resp.status.as_u16();
  if v.bool("require_login") == Some(true) {
    return Some(
      Error::auth(
        "Instagram answers this only to logged-in sessions (from this network, right now)",
      )
      .with_hint("run `atp instagram login --browser`"),
    );
  }
  if resp.url.contains("/accounts/login") || message == "login_required" || status == 401 {
    return Some(
      Error::auth("Instagram wants a logged-in session for this")
        .with_hint("run `atp instagram login --browser`"),
    );
  }
  if message.contains("checkpoint")
    || message.contains("challenge")
    || v.str("checkpoint_url").is_some()
  {
    return Some(
      Error::new(
        ErrorCode::VerificationRequired,
        "Instagram asks to confirm this account",
      )
      .with_hint("open instagram.com in the browser, pass the check, then try again later"),
    );
  }
  if status == 429 || message.contains("Please wait") || v.bool("spam") == Some(true) {
    return Some(
      Error::new(
        ErrorCode::RateLimited,
        "Instagram says to wait a few minutes",
      )
      .with_hint("slow down (--interval 10) and retry later"),
    );
  }
  None
}

/// A failed answer by status.
fn failure(status: u16, what: &str) -> Error {
  match status {
    403 => Error::new(
      ErrorCode::PermissionDenied,
      format!("Instagram refused: {what}"),
    ),
    400 => Error::input(format!("Instagram: {what}")),
    _ => Error::upstream(format!("Instagram ({status}): {what}")),
  }
}
