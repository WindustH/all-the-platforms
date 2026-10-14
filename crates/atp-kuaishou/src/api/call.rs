//! One request: its site, body and signature parameters.

use std::time::Duration;

use atp_core::http::{Method, status_error};
use atp_core::{Error, ErrorCode, Result, Value};

use super::check::check_live;
use super::{Api, CP_PH, RISK_HINT, Site, check};
use crate::device;
use crate::sign;
use crate::sign::falcon::Falcon;

enum Body {
  None,
  Json(Value),
  Form(Vec<(String, String)>),
}

/// One request under construction.
pub struct Call<'a> {
  api: &'a Api,
  site: Site,
  method: Method,
  path: String,
  query: Vec<(String, String)>,
  body: Body,
  referer: Option<String>,
  headers: Vec<(String, String)>,
  timeout: Option<Duration>,
}

impl<'a> Call<'a> {
  pub(super) fn new(api: &'a Api, site: Site, method: Method, path: &str) -> Self {
    Self {
      api,
      site,
      method,
      path: path.to_owned(),
      query: Vec::new(),
      body: Body::None,
      referer: None,
      headers: Vec::new(),
      timeout: None,
    }
  }

  pub fn arg(mut self, key: &str, value: impl ToString) -> Self {
    self.query.push((key.to_owned(), value.to_string()));
    self
  }

  pub fn json(mut self, body: Value) -> Self {
    self.body = Body::Json(body);
    self
  }

  pub fn form(mut self, fields: &[(&str, &str)]) -> Self {
    let fields = fields
      .iter()
      .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
      .collect();
    self.body = Body::Form(fields);
    self
  }

  /// The page the call comes from (and, for POSTs, its origin).
  pub fn referer(mut self, url: &str) -> Self {
    self.referer = Some(url.to_owned());
    self
  }

  pub fn header(mut self, name: &str, value: &str) -> Self {
    self.headers.push((name.to_owned(), value.to_owned()));
    self
  }

  pub fn timeout(mut self, timeout: Duration) -> Self {
    self.timeout = Some(timeout);
    self
  }

  /// Send and check the answer: www and cp `result`, live `data.result`
  /// (returning `data`). Login answers come back as they are.
  pub async fn send(self) -> Result<Value> {
    let site = self.site;
    let v = self.send_raw().await?;
    match site {
      Site::Www | Site::Cp => check(v),
      Site::Live => check_live(v),
      Site::Login => Ok(v),
    }
  }

  /// Send and return the JSON answer unchecked (but for HTTP errors).
  pub async fn send_raw(self) -> Result<Value> {
    let api = self.api;
    device::ensure(api).await;
    let body = self.body_text();
    let url = self.url(body.as_deref());
    let referer = self.referer.clone().unwrap_or_else(|| self.site.page());
    let mut req = api
      .ctx
      .http
      .request(self.method.clone(), url)
      .header("referer", &referer)
      .header("kww", api.kww(self.site))
      .header("sec-fetch-dest", "empty")
      .header("sec-fetch-mode", "cors")
      .header("sec-fetch-site", fetch_site(&referer, self.site));
    if self.method == Method::POST {
      req = req.header("origin", origin(&referer));
    }
    for (k, v) in &self.headers {
      req = req.header(k, v);
    }
    if let Some(text) = body {
      let kind = match self.body {
        Body::Form(_) => "application/x-www-form-urlencoded",
        _ => "application/json",
      };
      req = req.bytes(text.into_bytes(), kind);
    }
    if let Some(t) = self.timeout {
      req = req.timeout(t);
    }
    let resp = req.send().await?;
    tracing::debug!(
      "{}: {}",
      self.path,
      atp_core::text::truncate(&resp.text(), 300)
    );
    if !resp.status.is_success() {
      return Err(status_error(resp.status, &resp.text()).with_hint(RISK_HINT));
    }
    resp.value().map_err(|_| {
      Error::new(
        ErrorCode::VerificationRequired,
        "Kuaishou answered with a page instead of data",
      )
      .with_hint(RISK_HINT)
    })
  }

  /// The body as sent (and signed); cp bodies get `api_ph` first.
  fn body_text(&self) -> Option<String> {
    match &self.body {
      Body::None => None,
      Body::Form(fields) => Some(sign::form_string(fields)),
      Body::Json(v) => {
        let mut v = v.clone();
        if self.site == Site::Cp
          && let (Some(obj), Some(ph)) = (v.as_object_mut(), self.api.ctx.http.cookie(CP_PH))
        {
          obj.insert(CP_PH.into(), ph.into());
        }
        Some(v.to_string())
      }
    }
  }

  /// The URL with the signature parameters, then the call's query.
  fn url(&self, body: Option<&str>) -> String {
    let api = self.api;
    let path = self.path.as_str();
    let mut query = self.query.clone();
    let mut params: Vec<(String, String)> = Vec::new();
    let mut falcon = |signer: &Falcon, signed: &str, pairs: &[(String, String)], body| {
      let sig = signer.sign(&sign::falcon_input(signed, pairs, body));
      params.push(("__NS_hxfalcon".into(), sig));
      params.push(("caver".into(), sign::CAVER.into()));
    };
    match self.site {
      Site::Www if sign::www_signed(path) => {
        let body = body.filter(|b| !matches!(*b, "{}" | "null" | ""));
        falcon(&api.falcon, path, &query, body);
      }
      Site::Live => {
        if let Some(real) = sign::live_signed(path) {
          query.sort();
          falcon(&api.falcon_live, real, &query, Some("{}"));
        }
      }
      Site::Login if sign::login_signed(path) => {
        let Body::Form(fields) = &self.body else {
          unreachable!("login calls send forms")
        };
        falcon(&api.falcon_login, path, fields, Some("{}"));
      }
      Site::Cp if sign::sig3_signed(path) => {
        params.push(("__NS_sig3".into(), api.sig3.sign(body.unwrap_or("{}"))));
      }
      _ => {}
    }
    params.extend(query);
    let host = self.site.host();
    if params.is_empty() {
      format!("{host}{path}")
    } else {
      format!("{host}{path}?{}", sign::query_string(&params))
    }
  }
}

/// `https://host` of a page URL.
fn origin(url: &str) -> String {
  url.splitn(4, '/').take(3).collect::<Vec<_>>().join("/")
}

fn fetch_site(referer: &str, site: Site) -> &'static str {
  if origin(referer) == site.host() {
    "same-origin"
  } else {
    "same-site"
  }
}
