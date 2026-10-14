//! Pinduoduo transport. The H5 site `mobile.yangkeduo.com` renders pages
//! with their data in `window.rawData` and calls its API under
//! `/proxy/api` with an `anti_content` token ([`crate::sign`]); a login
//! (`PDDAccessToken`) unlocks everything beyond the home page. The
//! 多多进宝 open API is the other route ([`Ddk`]). Refusals map onto atp
//! errors: risk checks (`54001`) become `verification_required` with the
//! check page, login walls (`40001` without a session, `needLogin` pages)
//! `not_authenticated`, `40001` with a session `rate_limited`.

use atp_core::http::Resp;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

use crate::ddk::Ddk;
use crate::parse;
use crate::sign::{self, Signer};

pub const WEB: &str = "https://mobile.yangkeduo.com";
/// Cookies of a logged-in H5 session.
pub const LOGIN_COOKIES: &[&str] = &["PDDAccessToken", "pdd_user_id"];
const SERVER_TIME: &str = "/proxy/api/api/server/_stm";
const RISK_HINT: &str = "PDD's risk control wants a check: pass it in the page atp opens \
  (or open mobile.yangkeduo.com in a browser), then try again; keep requests sparse";

pub struct Api {
  pub ctx: Ctx,
  signer: Signer,
  pub ddk: Ddk,
}

impl Api {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("user-agent", sign::USER_AGENT);
    ctx.http.set_header("sec-ch-ua-mobile", "?1");
    ctx.http.set_header("sec-ch-ua-platform", "\"Android\"");
    let ddk = Ddk::from_env();
    Self {
      ctx,
      signer: Signer::default(),
      ddk,
    }
  }

  /// Whether the H5 session cookies are present.
  pub fn has_session(&self) -> bool {
    LOGIN_COOKIES.iter().all(|c| self.ctx.http.has_cookie(c))
  }

  /// The session's user id; `0` for visitors.
  pub fn uid(&self) -> String {
    self
      .ctx
      .http
      .cookie("pdd_user_id")
      .unwrap_or_else(|| "0".into())
  }

  /// How to get either route: DDK credentials or an H5 login.
  pub fn either_hint(&self) -> String {
    format!(
      "{}; or log in to the H5 site with `atp pdd login --browser` (or --cookie)",
      crate::ddk::LOGIN_HINT
    )
  }

  pub fn require_session(&self) -> Result<()> {
    if self.has_session() {
      return Ok(());
    }
    Err(
      Error::auth("this needs a Pinduoduo login (cookies PDDAccessToken and pdd_user_id)")
        .with_hint(self.ctx.login_hint()),
    )
  }

  /// The `window.rawData` store of a server-rendered page.
  pub async fn store(&self, url: &str) -> Result<Value> {
    let resp = self
      .ctx
      .http
      .get(url)
      .header(
        "accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
      )
      .header("sec-fetch-mode", "navigate")
      .header("sec-fetch-dest", "document")
      .send()
      .await?;
    if let Some(e) = page_redirect(&resp, self) {
      return Err(e);
    }
    let html = resp.check()?.text();
    let store =
      parse::raw_data(&html).ok_or_else(|| Error::upstream(format!("no page data in {url}")))?;
    if store.bool("initDataObj.needLogin") == Some(true) {
      return Err(
        Error::auth("Pinduoduo shows this page after a login").with_hint(self.ctx.login_hint()),
      );
    }
    if let Some(time) = store.i64("common.nodeServerTime") {
      self.start(url, time);
    }
    Ok(store)
  }

  /// Load the home page once when the session has no visitor id
  /// (`api_uid`), as a browser would have before calling from it.
  pub async fn visit(&self) -> Result<()> {
    if !self.ctx.http.has_cookie("api_uid") {
      self.store(&format!("{WEB}/")).await?;
    }
    Ok(())
  }

  /// A signed GET of `/proxy/api<path>` from the page `from`.
  pub async fn get(&self, path: &str, params: &[(&str, String)], from: &str) -> Result<Value> {
    let token = self.token(from).await;
    let mut params = params.to_vec();
    params.push(("anti_content", token));
    self.get_plain(path, &params, from).await
  }

  /// A GET of `/proxy/api<path>` without a token (calls pages send bare).
  pub async fn get_plain(
    &self,
    path: &str,
    params: &[(&str, String)],
    from: &str,
  ) -> Result<Value> {
    let mut query: Vec<(&str, String)> = vec![("pdduid", self.uid())];
    query.extend(params.iter().cloned());
    let req = self
      .ctx
      .http
      .get(format!("{WEB}/proxy/api{path}"))
      .queries(query);
    let resp = self.xhr(req, from).send().await?;
    self.check(&resp, path)
  }

  /// A signed JSON POST of `/proxy/api<path>`; the token goes in the body.
  pub async fn post(&self, path: &str, mut body: Value, from: &str) -> Result<Value> {
    body["anti_content"] = json!(self.token(from).await);
    let url = format!("{WEB}/proxy/api{path}");
    let req = self
      .ctx
      .http
      .post(url)
      .query("pdduid", self.uid())
      .json(&body)
      .header("origin", WEB);
    let resp = self.xhr(req, from).send().await?;
    self.check(&resp, path)
  }

  fn xhr<'a>(&self, req: atp_core::http::Req<'a>, from: &str) -> atp_core::http::Req<'a> {
    let mut req = req
      .header("accept", "application/json, text/plain, */*")
      .header("referer", from)
      .header("sec-fetch-site", "same-origin")
      .header("sec-fetch-mode", "cors")
      .header("sec-fetch-dest", "empty");
    if let Some(token) = self.ctx.http.cookie("PDDAccessToken") {
      req = req.header("accesstoken", token);
    }
    req
  }

  /// An `anti_content` token; the first one loads the generator's page
  /// with the server time.
  async fn token(&self, from: &str) -> String {
    if !self.signer.started() {
      let time = self.server_time().await;
      self.start(from, time);
    }
    self.signer.token(&self.ctx, "")
  }

  fn start(&self, href: &str, server_time: i64) {
    if !self.signer.started() {
      self.signer.start(href, server_time);
    }
  }

  /// The server's clock (ms), or the local one when it cannot be read.
  async fn server_time(&self) -> i64 {
    let local = jiff::Timestamp::now().as_millisecond();
    let resp = self
      .ctx
      .http
      .get(format!("{WEB}{SERVER_TIME}"))
      .header("referer", format!("{WEB}/"))
      .send()
      .await;
    let time = resp
      .ok()
      .and_then(|r| r.value().ok())
      .and_then(|v| v.i64("server_time"));
    match time {
      Some(t) if t < 10_000_000_000 => t * 1000,
      Some(t) => t,
      None => local,
    }
  }

  /// PDD's error envelope (`error_code`, `error_msg`) mapped onto atp's.
  fn check(&self, resp: &Resp, path: &str) -> Result<Value> {
    let v = match resp.value() {
      Ok(v) => v,
      Err(_) if resp.status.as_u16() == 429 => return Err(throttled(path)),
      Err(_) => return Err(atp_core::http::status_error(resp.status, &resp.text())),
    };
    let code = v.i64("error_code").or_else(|| v.i64("errorCode"));
    let mut message = v
      .first_str(&["error_msg", "errorMsg", "message"])
      .map(|m| format!(" ({m})"))
      .unwrap_or_default();
    if let Some(judgement) = v.str("judgement") {
      message.push_str(&format!(" (risk judgement {judgement})"));
    }
    match code {
      None | Some(0) | Some(1_000_000) if resp.status.is_success() => Ok(v),
      Some(54001) => Err(verification(&v)),
      Some(40001) if !self.has_session() => Err(
        Error::auth(format!("Pinduoduo wants a login for {path}{message}"))
          .with_hint(self.ctx.login_hint()),
      ),
      Some(40001) | Some(42001) => Err(throttled(path).with_hint(format!(
        "PDD answered busy{message}: wait a while; if it persists the session may \
         have expired ({})",
        self.ctx.login_hint()
      ))),
      Some(40003) => Err(Error::new(
        ErrorCode::SignatureError,
        format!("Pinduoduo refused the request parameters of {path}{message}"),
      )),
      Some(43001) | Some(43002) => Err(
        Error::auth(format!("the Pinduoduo session has expired{message}"))
          .with_hint(self.ctx.login_hint()),
      ),
      _ if resp.status.as_u16() == 429 => Err(throttled(path)),
      code => Err(Error::upstream(format!(
        "Pinduoduo {path} failed{message} (code {})",
        code.map_or_else(|| resp.status.as_u16().to_string(), |c| c.to_string())
      ))),
    }
  }
}

fn throttled(path: &str) -> Error {
  Error::new(
    ErrorCode::RateLimited,
    format!("Pinduoduo is throttling {path}"),
  )
}

/// `54001`: a check the page would open (`psnl_verification.html`).
fn verification(v: &Value) -> Error {
  let token = v
    .first_str(&[
      "verify_auth_token",
      "verifyAuthToken",
      "result.verify_auth_token",
    ])
    .unwrap_or_default();
  let mut error = Error::new(
    ErrorCode::VerificationRequired,
    "Pinduoduo asks for a verification (54001)",
  )
  .with_hint(RISK_HINT);
  if !token.is_empty() {
    let mut url = url::Url::parse(&format!("{WEB}/psnl_verification.html")).expect("valid");
    url.query_pairs_mut().append_pair("VerifyAuthToken", &token);
    error = error.with_url(url.as_str());
  }
  error
}

/// Pages that redirect to the login or a check page instead of rendering.
fn page_redirect(resp: &Resp, api: &Api) -> Option<Error> {
  let url = &resp.url;
  if url.contains("login.html") {
    return Some(
      Error::auth("Pinduoduo sent the page to its login").with_hint(api.ctx.login_hint()),
    );
  }
  (url.contains("verification.html") || url.contains("psnl_verification")).then(|| {
    Error::new(
      ErrorCode::VerificationRequired,
      "Pinduoduo asks for a verification",
    )
    .with_url(url)
    .with_hint(RISK_HINT)
  })
}

/// A list's `list_id` as the pages make it: ten random letters and digits.
pub fn list_id() -> String {
  use rand::Rng;
  const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
  let mut rng = rand::rng();
  (0..10)
    .map(|_| CHARS[rng.random_range(0..CHARS.len())] as char)
    .collect()
}

/// A goods page.
pub fn goods_url(id: &str) -> String {
  format!("{WEB}/goods.html?goods_id={id}")
}

/// The search page for a keyword.
pub fn search_url(keyword: &str) -> String {
  format!(
    "{WEB}/search_result.html?search_key={}",
    atp_core::text::encode_uri_component(keyword)
  )
}

/// A shop page.
pub fn mall_url(id: &str) -> String {
  format!("{WEB}/mall_page.html?mall_id={id}")
}

/// Image links as the pages use them: `//host/...` and `http:` made https.
pub fn image_url(url: &str) -> String {
  let url = url.trim();
  if let Some(rest) = url.strip_prefix("//") {
    format!("https://{rest}")
  } else {
    url.replacen("http://", "https://", 1)
  }
}
