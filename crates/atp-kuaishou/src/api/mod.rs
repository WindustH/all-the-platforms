//! Kuaishou web transport. Four sites answer: www (REST under `/rest/v/`
//! and `/graphql`), live (`/live_api/`), the creator center (cp) and the
//! login site (id.kuaishou.com). A [`Call`] is signed the way its page
//! signs it (see [`crate::sign`]), carries the `kww` header, and its answer
//! is checked: `result` 1 is fine, 109 wants a login, 2 is risk control,
//! 400002 a slider captcha.

use std::cell::{Cell, OnceCell};

use atp_core::http::{Method, status_error};
use atp_core::{Ctx, Result, Value};

use crate::device;
use crate::sign::falcon::{Falcon, SDK_LIVE, SDK_WWW};
use crate::sign::sig3::Sig3;
use crate::sign::{self, weapon};

mod call;
mod check;

pub use self::call::Call;
pub use self::check::check;
use self::check::check_graphql;

pub const WWW: &str = "https://www.kuaishou.com";
pub const LIVE: &str = "https://live.kuaishou.com";
pub const CP: &str = "https://cp.kuaishou.com";
pub const ID: &str = "https://id.kuaishou.com";
/// The mobile share pages.
pub const H5: &str = "https://v.m.chenzhongtech.com";
/// Cookies of a logged-in session.
pub const LOGIN_COOKIES: &[&str] = &["userId"];
/// The creator center's CSRF value, added to every cp body.
const CP_PH: &str = "kuaishou.web.cp.api_ph";

pub const LOGIN_HINT: &str =
  "log in with `atp kuaishou login` (QR code) or `atp kuaishou login --browser`";
const CAPTCHA_HINT: &str = "open the link (or kuaishou.com) in a browser and pass the slider, \
   then import that browser's cookies with `atp kuaishou login --browser`; or wait a while";
const RISK_HINT: &str = "Kuaishou flags fast or cookie-less clients: wait a while, or log in \
   with `atp kuaishou login --browser`";
pub const DOC_ACCEPT: &str =
  "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8";
const MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) \
   AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";

/// The transport: the context plus one signer per page kind, like the
/// browser tabs a user would have open.
pub struct Api {
  pub ctx: Ctx,
  falcon: Falcon,
  falcon_live: Falcon,
  falcon_login: Falcon,
  sig3: Sig3,
  kww_live: OnceCell<String>,
  kww_cp: OnceCell<String>,
  /// QR login: the code was scanned and the confirmation is pending.
  pub scanned: Cell<bool>,
  /// The account's eid, once a call needed it.
  pub me: OnceCell<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Site {
  Www,
  Live,
  Cp,
  Login,
}

impl Site {
  fn host(self) -> &'static str {
    match self {
      Site::Www => WWW,
      Site::Live => LIVE,
      Site::Cp => CP,
      Site::Login => ID,
    }
  }

  /// The page a call of this site comes from by default.
  fn page(self) -> String {
    match self {
      Site::Www => format!("{WWW}/new-reco"),
      Site::Live => format!("{LIVE}/"),
      Site::Cp => format!("{CP}/profile"),
      Site::Login => format!("{WWW}/"),
    }
  }
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      falcon: Falcon::new(SDK_WWW),
      falcon_live: Falcon::new(SDK_LIVE),
      falcon_login: Falcon::new(SDK_WWW),
      sig3: Sig3::start(),
      kww_live: OnceCell::new(),
      kww_cp: OnceCell::new(),
      scanned: Cell::new(false),
      me: OnceCell::new(),
    }
  }

  /// A www REST call (POST; a JSON body when [`Call::json`] gives one).
  pub fn www(&self, path: &str) -> Call<'_> {
    Call::new(self, Site::Www, Method::POST, path)
  }

  pub fn www_get(&self, path: &str) -> Call<'_> {
    Call::new(self, Site::Www, Method::GET, path)
  }

  pub fn live(&self, path: &str) -> Call<'_> {
    Call::new(self, Site::Live, Method::GET, path)
  }

  pub fn live_post(&self, path: &str) -> Call<'_> {
    Call::new(self, Site::Live, Method::POST, path)
  }

  /// A creator-center call: a JSON body (`{}` unless given) with `api_ph`.
  pub fn cp(&self, path: &str) -> Call<'_> {
    Call::new(self, Site::Cp, Method::POST, path).json(Value::Object(Default::default()))
  }

  /// A login-site call with a form body.
  pub fn login(&self, path: &str) -> Call<'_> {
    Call::new(self, Site::Login, Method::POST, path)
  }

  /// Fail with `not_authenticated` unless the session cookie is present.
  pub fn require_login(&self) -> Result<()> {
    self.ctx.require_login(LOGIN_COOKIES)
  }

  /// A GraphQL operation of the www site; returns its `data`. The server
  /// checks the query text, so callers pass the browser's verbatim.
  pub async fn graphql(
    &self,
    operation: &str,
    variables: Value,
    query: &str,
    referer: &str,
  ) -> Result<Value> {
    let body = atp_core::json!({
      "operationName": operation,
      "variables": variables,
      "query": query,
    });
    let v = self
      .www("/graphql")
      .json(body)
      .referer(referer)
      .header("accept", "*/*")
      .send_raw()
      .await?;
    check_graphql(v)
  }

  /// An HTML page, as a browser navigating to it (`mobile`: as a phone).
  pub async fn page(&self, url: &str, mobile: bool) -> Result<String> {
    device::ensure(self).await;
    let mut req = self
      .ctx
      .http
      .get(url)
      .header("accept", DOC_ACCEPT)
      .header("sec-fetch-dest", "document")
      .header("sec-fetch-mode", "navigate")
      .header("sec-fetch-site", "none");
    if mobile {
      req = req.header("user-agent", MOBILE_UA);
    }
    let resp = req.send().await?;
    if !resp.status.is_success() {
      return Err(status_error(resp.status, &resp.text()).with_hint(RISK_HINT));
    }
    Ok(resp.text())
  }

  /// The `kww` header: the session's `kwfv1` on www, a first-visit value
  /// of the site's product elsewhere.
  fn kww(&self, site: Site) -> String {
    let http = &self.ctx.http;
    let did = http.cookie("did").unwrap_or_default();
    let make = |href: &str, product: &str| weapon::kwfv1(href, &did, product, sign::now_ms());
    match site {
      Site::Live => self
        .kww_live
        .get_or_init(|| make(&format!("{LIVE}/"), weapon::PRODUCT_LIVE))
        .clone(),
      Site::Cp => self
        .kww_cp
        .get_or_init(|| make(&format!("{CP}/profile"), weapon::PRODUCT_CP))
        .clone(),
      Site::Www | Site::Login => http.cookie("kwfv1").unwrap_or_default(),
    }
  }
}
