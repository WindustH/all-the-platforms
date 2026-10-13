//! Reddit transport. The one place that knows which of two modes is active:
//!
//! - **Web** (default): `https://www.reddit.com/<path>.json?raw_json=1`, with
//!   the browser session cookies (`reddit_session`) when logged in and
//!   anonymously otherwise. Writes use the web app's own bearer token, the
//!   `token_v2` cookie (a JWT living about a day, renewed by loading the home
//!   page), against `oauth.reddit.com`: the classic cookie API answers
//!   submissions with `BAD_CAPTCHA`. Without a usable `token_v2`, writes fall
//!   back to the classic API on www with the account's `modhash` (from
//!   `/api/me.json`) as `X-Modhash` header and `uh` field.
//! - **OAuth**: with `REDDIT_CLIENT_ID`, `REDDIT_CLIENT_SECRET`,
//!   `REDDIT_USERNAME` and `REDDIT_PASSWORD` set, a password-grant token
//!   ([`crate::oauth`]) goes as `Authorization: bearer` to
//!   `https://oauth.reddit.com/<path>`. It wins over saved cookies.
//!
//! Paths are the same in both modes, so callers only name the endpoint.
//! Failures are mapped to `atp_core::Error`: 429, Reddit's "blocked by
//! network security" page, private / quarantined / banned communities and the
//! `{"json": {"errors": [...]}}` envelope of writes.

mod check;
mod session;

use std::cell::RefCell;
use std::time::Duration;

use atp_core::http::{Method, Req};
use atp_core::{Ctx, Error, Result, Value};

use self::check::api_errors;

use crate::oauth::Credentials;

pub const WWW: &str = "https://www.reddit.com";
const OAUTH: &str = "https://oauth.reddit.com";
/// Cookie of a logged-in browser session.
pub const SESSION_COOKIE: &str = "reddit_session";
/// Session extra holding the account name, so listings of your own saved /
/// upvoted posts need no extra lookup.
const USER_KEY: &str = "username";
const BLOCK_HINT: &str = "Reddit refuses anonymous API requests from some networks: log in \
   (`atp reddit login --browser`), set REDDIT_CLIENT_ID / REDDIT_CLIENT_SECRET / \
   REDDIT_USERNAME / REDDIT_PASSWORD, or retry through another network with --proxy";

/// Query parameters or form fields.
pub type Params<'a> = Vec<(&'a str, String)>;

enum Body<'a> {
  Form(Params<'a>),
  Json(Value),
}

pub struct Api {
  pub ctx: Ctx,
  oauth: Option<Credentials>,
  modhash: RefCell<Option<String>>,
}

impl Api {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("accept-language", "en-US,en;q=0.9");
    Self {
      ctx,
      oauth: Credentials::from_env(),
      modhash: RefCell::new(None),
    }
  }

  pub fn logged_in(&self) -> bool {
    self.oauth.is_some() || self.ctx.http.has_cookie(SESSION_COOKIE)
  }

  pub fn require_login(&self) -> Result<()> {
    match self.oauth {
      Some(_) => Ok(()),
      None => self.ctx.require_login(&[SESSION_COOKIE]),
    }
  }

  /// GET an endpoint given without host and `.json` (`/r/rust/hot`).
  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let resp = self
      .request(Method::GET, path)
      .await?
      .queries(query.iter().map(|(k, v)| (*k, v)))
      .query("raw_json", 1)
      .send()
      .await?;
    // Unknown communities redirect to a community search.
    if path.starts_with("/r/") && resp.url.contains("/subreddits/search") {
      let name = path.split('/').nth(2).unwrap_or_default();
      return Err(Error::not_found(format!("no such subreddit: r/{name}")));
    }
    self.check(resp)
  }

  /// GET an endpoint of `oauth.reddit.com` only (`/api/v1/me/karma`): with
  /// the web app's bearer on a cookie session, like [`Api::get`] otherwise.
  pub async fn get_oauth(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let Some(token) = self.web_token().await else {
      return self.get(path, query).await;
    };
    let req = self.bearer(Method::GET, path, &token);
    let req = req.queries(query.iter().map(|(k, v)| (*k, v)));
    self.check(req.query("raw_json", 1).send().await?)
  }

  /// POST a form to the classic API: a write (login, pause, CSRF, `api_type=json`).
  pub async fn post(&self, path: &str, form: Params<'_>) -> Result<Value> {
    self.write(path, Body::Form(form)).await
  }

  /// POST a JSON body (endpoints such as `/api/submit_gallery_post.json`).
  pub async fn post_json(&self, path: &str, body: Value) -> Result<Value> {
    self.write(path, Body::Json(body)).await
  }

  async fn write(&self, path: &str, body: Body<'_>) -> Result<Value> {
    self.require_login()?;
    match self.send_write(path, &body, self.web_token().await).await {
      // The classic cookie API wants a captcha for some writes (submissions);
      // its responses often hand out the `token_v2` bearer, so retry with it.
      // Nothing was created by the refused attempt.
      Err(e) if e.message.contains("BAD_CAPTCHA") => match self.web_token().await {
        Some(token) => self.send_write(path, &body, Some(token)).await,
        None => Err(e),
      },
      other => other,
    }
  }

  /// One write: with the web bearer when given, else with the active mode.
  async fn send_write(&self, path: &str, body: &Body<'_>, token: Option<String>) -> Result<Value> {
    let (mut req, modhash) = match token {
      Some(token) => (self.bearer(Method::POST, path, &token), None),
      None => {
        let modhash = match self.oauth {
          Some(_) => None,
          None => Some(self.modhash().await?),
        };
        (self.request(Method::POST, path).await?, modhash)
      }
    };
    req = match body {
      Body::Json(v) => req.json(v),
      Body::Form(form) => {
        let mut form = form.clone();
        form.push(("api_type", "json".into()));
        if let Some(m) = &modhash {
          form.push(("uh", m.clone()));
        }
        req.form(form)
      }
    };
    if let Some(m) = &modhash {
      req = req.header("x-modhash", m);
    }
    self
      .ctx
      .http
      .pause(Duration::from_millis(600), Duration::from_millis(1800))
      .await;
    let v = self.check(req.send().await?)?;
    api_errors(&v)?;
    Ok(v)
  }

  /// A request to the active host with the active credentials.
  async fn request(&self, method: Method, path: &str) -> Result<Req<'_>> {
    let http = &self.ctx.http;
    if let Some(o) = &self.oauth {
      return Ok(
        self
          .bearer(method, path, &o.token(&self.ctx).await?)
          .header("user-agent", o.user_agent()),
      );
    }
    Ok(if method == Method::GET {
      http.get(format!("{WWW}{}.json", path.trim_end_matches('/')))
    } else {
      http
        .request(method, format!("{WWW}{path}"))
        .header("origin", WWW)
        .header("referer", format!("{WWW}/"))
    })
  }

  /// A request to `oauth.reddit.com` carrying `token`.
  fn bearer(&self, method: Method, path: &str, token: &str) -> Req<'_> {
    self
      .ctx
      .http
      .request(method, format!("{OAUTH}{path}"))
      .no_cookies()
      .header("authorization", format!("bearer {token}"))
  }
}
