//! Shared HTTP transport.
//!
//! One Chrome-fingerprinted client per platform with its own cookie jar,
//! request throttling, retry with backoff and streaming downloads. Platforms
//! build requests with [`Http::get`] / [`Http::post`] and interpret the
//! response envelope themselves.

mod request;
mod response;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use rand::Rng;
use wreq::header::{HeaderMap, HeaderName, HeaderValue};
pub use wreq::{Method, StatusCode};

pub use self::request::*;
pub use self::response::*;

/// Chrome build the TLS/HTTP2 fingerprint imitates; keep headers consistent with it.
pub const CHROME_VERSION: u32 = 149;

pub const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36";

pub const SEC_CH_UA: &str = r#""Google Chrome";v="149", "Chromium";v="149", "Not)A;Brand";v="24""#;

pub const SEC_CH_UA_PLATFORM: &str = r#""macOS""#;

pub type Cookies = BTreeMap<String, String>;

#[derive(Debug, Clone)]
pub struct HttpConfig {
  pub proxy: Option<String>,
  pub timeout: Duration,
  /// Minimum gap between two throttled requests (jitter is added on top).
  pub min_interval: Duration,
}

impl Default for HttpConfig {
  fn default() -> Self {
    Self {
      proxy: None,
      timeout: Duration::from_secs(30),
      min_interval: Duration::ZERO,
    }
  }
}

pub struct Http {
  client: wreq::Client,
  headers: HeaderMap,
  jar: RefCell<Cookies>,
  dirty: Cell<bool>,
  last: Cell<Option<Instant>>,
  min_interval: Duration,
  proxy: Option<String>,
}

impl Http {
  pub fn new(cfg: &HttpConfig, cookies: Cookies) -> Result<Self> {
    let emulation = wreq_util::Emulation::builder()
      .profile(wreq_util::Profile::Chrome149)
      .platform(wreq_util::Platform::MacOS)
      .headers(false)
      .build();
    let mut builder = wreq::Client::builder()
      .emulation(emulation)
      .timeout(cfg.timeout)
      .connect_timeout(Duration::from_secs(15))
      .redirect(wreq::redirect::Policy::limited(10));
    if let Some(proxy) = &cfg.proxy {
      builder = builder.proxy(
        wreq::Proxy::all(proxy.as_str()).map_err(|e| Error::input(format!("bad proxy: {e}")))?,
      );
    }
    let client = builder
      .build()
      .map_err(|e| Error::internal(format!("http client: {e}")))?;

    let mut headers = HeaderMap::new();
    for (k, v) in [
      ("user-agent", USER_AGENT),
      ("sec-ch-ua", SEC_CH_UA),
      ("sec-ch-ua-mobile", "?0"),
      ("sec-ch-ua-platform", SEC_CH_UA_PLATFORM),
      ("accept", "application/json, text/plain, */*"),
      ("accept-language", "zh-CN,zh;q=0.9,en;q=0.8"),
    ] {
      headers.insert(HeaderName::from_static(k), HeaderValue::from_static(v));
    }
    Ok(Self {
      client,
      headers,
      jar: RefCell::new(cookies),
      dirty: Cell::new(false),
      last: Cell::new(None),
      min_interval: cfg.min_interval,
      proxy: cfg.proxy.clone(),
    })
  }

  /// Set (or with an empty value, remove) a header sent with every request.
  pub fn set_header(&mut self, name: &str, value: &str) {
    let name = HeaderName::from_bytes(name.as_bytes()).expect("valid header name");
    if value.is_empty() {
      self.headers.remove(name);
    } else {
      self.headers.insert(
        name,
        HeaderValue::from_str(value).expect("valid header value"),
      );
    }
  }

  pub fn get(&self, url: impl Into<String>) -> Req<'_> {
    self.request(Method::GET, url)
  }

  pub fn post(&self, url: impl Into<String>) -> Req<'_> {
    self.request(Method::POST, url)
  }

  pub fn request(&self, method: Method, url: impl Into<String>) -> Req<'_> {
    Req::new(self, method, url.into())
  }

  /// Where a (short) link finally leads, following redirects without cookies.
  /// Tries HEAD first and falls back to GET for servers that refuse HEAD.
  pub async fn final_url(&self, url: &str) -> Result<String> {
    let head = self.request(Method::HEAD, url).no_cookies().send().await;
    match head {
      Ok(r) if r.status.is_success() => Ok(r.url),
      _ => Ok(self.get(url).no_cookies().send().await?.url),
    }
  }

  // ── cookie jar ────────────────────────────────────────────────────────

  pub fn cookie(&self, name: &str) -> Option<String> {
    self
      .jar
      .borrow()
      .get(name)
      .filter(|v| !v.is_empty())
      .cloned()
  }

  pub fn has_cookie(&self, name: &str) -> bool {
    self.cookie(name).is_some()
  }

  pub fn set_cookie(&self, name: &str, value: &str) {
    let mut jar = self.jar.borrow_mut();
    if jar.get(name).map(String::as_str) != Some(value) {
      jar.insert(name.to_owned(), value.to_owned());
      self.dirty.set(true);
    }
  }

  pub fn remove_cookie(&self, name: &str) {
    if self.jar.borrow_mut().remove(name).is_some() {
      self.dirty.set(true);
    }
  }

  pub fn cookies(&self) -> Cookies {
    self.jar.borrow().clone()
  }

  /// Replace the whole jar (after a login).
  pub fn replace_cookies(&self, cookies: Cookies) {
    *self.jar.borrow_mut() = cookies;
    self.dirty.set(true);
  }

  /// `a=1; b=2` for the current jar.
  /// The proxy for connections this client cannot carry itself (MTProto,
  /// third-party websockets): `--proxy`, then `SOCIAL_PROXY`, `ALL_PROXY`,
  /// `HTTPS_PROXY` (either case).
  pub fn proxy(&self) -> Option<String> {
    let var = |name: &str| {
      std::env::var(name)
        .or_else(|_| std::env::var(name.to_ascii_lowercase()))
        .ok()
        .filter(|v| !v.trim().is_empty())
    };
    self
      .proxy
      .clone()
      .or_else(|| var("SOCIAL_PROXY"))
      .or_else(|| var("ALL_PROXY"))
      .or_else(|| var("HTTPS_PROXY"))
  }

  pub(crate) fn client(&self) -> &wreq::Client {
    &self.client
  }

  pub fn cookie_header(&self) -> String {
    self
      .jar
      .borrow()
      .iter()
      .map(|(k, v)| format!("{k}={v}"))
      .collect::<Vec<_>>()
      .join("; ")
  }

  /// Whether the jar changed since the session was loaded.
  pub fn cookies_changed(&self) -> bool {
    self.dirty.get()
  }

  // ── pacing ────────────────────────────────────────────────────────────

  /// Sleep a random duration in `[min, max]`; used before write operations.
  pub async fn pause(&self, min: Duration, max: Duration) {
    let ms = rand::rng()
      .random_range(min.as_millis() as u64..=max.as_millis().max(min.as_millis()) as u64);
    tokio::time::sleep(Duration::from_millis(ms)).await;
  }

  async fn throttle(&self) {
    if self.min_interval.is_zero() {
      return;
    }
    if let Some(last) = self.last.get() {
      let elapsed = last.elapsed();
      if elapsed < self.min_interval {
        let mut rng = rand::rng();
        // Mostly short jitter, occasionally a longer "reading" pause.
        let mut jitter = rng.random_range(100..500);
        if rng.random_bool(0.05) {
          jitter += rng.random_range(2000..5000);
        }
        tokio::time::sleep(self.min_interval - elapsed + Duration::from_millis(jitter)).await;
      }
    }
  }

  fn absorb_cookies(&self, resp: &wreq::Response) {
    let mut jar = self.jar.borrow_mut();
    for c in resp.cookies() {
      // Names only: values are credentials.
      tracing::trace!("set-cookie {}", c.name());
      let expired = c.max_age().is_some_and(|a| a.is_zero());
      if c.value().is_empty() || c.value() == "deleted" || expired {
        if jar.remove(c.name()).is_some() {
          self.dirty.set(true);
        }
      } else if jar.get(c.name()).map(String::as_str) != Some(c.value()) {
        jar.insert(c.name().to_owned(), c.value().to_owned());
        self.dirty.set(true);
      }
    }
  }
}
