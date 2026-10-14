//! The one path every request to Meituan's and Dianping's gateway takes.
//!
//! A [`Call`] says what a page of the site would send: the URL and query,
//! the page it comes from (the `Referer`, and the page to open when the
//! gateway refuses), and whether the page adds the H5 `csec` flags and the
//! rohr `_token`. [`send`] adds those, sends with the session's cookies and
//! turns refusals into errors ([`crate::refusal`]). A request signer, if
//! one is ever added, belongs here; atp sends no `mtgsig`.

use atp_core::http::{Req, Resp, status_error};
use atp_core::{Ctx, Result, Value};

use crate::{refusal, rohr};

/// One of the two sites: its name in messages, home page and rohr
/// business id.
#[derive(Debug, Clone, Copy)]
pub struct Site {
  pub name: &'static str,
  pub home: &'static str,
  pub rohr_id: u32,
}

pub const DIANPING: Site = Site {
  name: "Dianping",
  home: "https://www.dianping.com/",
  rohr_id: 100041,
};

pub const MEITUAN: Site = Site {
  name: "Meituan",
  home: "https://www.meituan.com/",
  rohr_id: 100900,
};

/// The H5 flags (`csecplatform` 4 is the web; the apps' values never go
/// out with web requests).
const CSEC: [(&str, &str); 3] = [
  ("yodaReady", "h5"),
  ("csecplatform", "4"),
  ("csecversion", "4.3.0"),
];

/// Chrome on Android, for the mobile site's pages: the same browser as
/// atp's client, on a phone.
pub const MOBILE_UA: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 \
  (KHTML, like Gecko) Chrome/149.0.0.0 Mobile Safari/537.36";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  /// A page the browser navigates to.
  Page,
  /// A script's request from a page.
  Xhr,
}

/// A GET as a page of the site sends it.
#[derive(Debug, Clone)]
pub struct Call {
  pub url: String,
  pub query: Vec<(String, String)>,
  /// The page it comes from: the `Referer`, and the page to open when the
  /// gateway refuses.
  pub from: String,
  pub kind: Kind,
  /// Add the rohr `_token` over the query.
  pub token: bool,
  /// Add the H5 `csec` flags.
  pub csec: bool,
  /// Present a phone (the mobile site).
  pub mobile: bool,
}

impl Call {
  pub fn page(url: impl Into<String>, from: impl Into<String>) -> Self {
    Self::new(Kind::Page, url.into(), from.into())
  }

  pub fn xhr(url: impl Into<String>, from: impl Into<String>) -> Self {
    Self::new(Kind::Xhr, url.into(), from.into())
  }

  fn new(kind: Kind, url: String, from: String) -> Self {
    Self {
      url,
      query: Vec::new(),
      from,
      kind,
      token: false,
      csec: false,
      mobile: false,
    }
  }

  pub fn query(mut self, key: &str, value: impl ToString) -> Self {
    self.query.push((key.to_owned(), value.to_string()));
    self
  }

  /// The H5 page's flags and its rohr `_token`.
  pub fn h5(mut self) -> Self {
    self.csec = true;
    self.token = true;
    self
  }

  pub fn mobile(mut self) -> Self {
    self.mobile = true;
    self
  }
}

/// Send `call`; refusals (a check, a login page, the gateway's 403) come
/// back as errors, everything else as it is.
pub async fn send(ctx: &Ctx, site: &Site, call: Call) -> Result<Resp> {
  let resp = request(ctx, site, &call).send().await?;
  match refusal::of_response(ctx, site, &resp, &call.from) {
    Some(e) => Err(e),
    None => Ok(resp),
  }
}

/// [`send`], then the JSON answer, refusing the ones that name a check, a
/// login wall or account risk.
pub async fn json(ctx: &Ctx, site: &Site, call: Call) -> Result<Value> {
  let from = call.from.clone();
  let resp = send(ctx, site, call).await?;
  let parsed = resp.value();
  if let Ok(v) = &parsed
    && let Some(e) = refusal::of_body(ctx, site, v, &from)
  {
    return Err(e);
  }
  if !resp.status.is_success() {
    return Err(status_error(resp.status, &resp.text()));
  }
  parsed
}

/// [`send`], then the page's HTML.
pub async fn html(ctx: &Ctx, site: &Site, call: Call) -> Result<String> {
  Ok(send(ctx, site, call).await?.check()?.text())
}

fn request<'a>(ctx: &'a Ctx, site: &Site, call: &Call) -> Req<'a> {
  let mut query = call.query.clone();
  if call.csec {
    query.extend(CSEC.map(|(k, v)| (k.to_owned(), v.to_owned())));
  }
  if call.token {
    let token = rohr::token(site.rohr_id, &call.from, &query);
    query.push(("_token".to_owned(), token));
  }
  let mut req = ctx
    .http
    .get(&call.url)
    .queries(query)
    .header("referer", &call.from);
  req = match call.kind {
    Kind::Page => req
      .header(
        "accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
      )
      .header("sec-fetch-mode", "navigate")
      .header("sec-fetch-dest", "document"),
    Kind::Xhr => req
      .header("accept", "application/json, text/plain, */*")
      .header("origin", origin(&call.from))
      .header("sec-fetch-mode", "cors")
      .header("sec-fetch-dest", "empty"),
  };
  if call.mobile {
    req = req
      .header("user-agent", MOBILE_UA)
      .header("sec-ch-ua-mobile", "?1")
      .header("sec-ch-ua-platform", "\"Android\"");
  }
  req
}

/// `https://host` of a page.
fn origin(page: &str) -> String {
  url::Url::parse(page)
    .map(|u| u.origin().ascii_serialization())
    .unwrap_or_else(|_| page.trim_end_matches('/').to_owned())
}
