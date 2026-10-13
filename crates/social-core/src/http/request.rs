//! Requests under construction, and how they are sent: pacing, retries,
//! streaming.

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use rand::Rng;
use serde::Serialize;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use wreq::header::{HeaderMap, HeaderName, HeaderValue};

use super::{Fetched, Http, Method, Resp, StatusCode, status_error};
use crate::error::{Error, Result};

#[derive(Clone)]
enum Body {
  Empty,
  Bytes(Vec<u8>, String),
  Multipart(Vec<Part>),
}

/// One field of a multipart form.
#[derive(Clone)]
pub struct Part {
  pub name: String,
  pub data: Vec<u8>,
  pub file_name: Option<String>,
  pub mime: Option<String>,
}

impl Part {
  pub fn text(name: &str, value: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      data: value.into().into_bytes(),
      file_name: None,
      mime: None,
    }
  }

  pub fn file(name: &str, data: Vec<u8>, file_name: &str, mime: &str) -> Self {
    Self {
      name: name.into(),
      data,
      file_name: Some(file_name.into()),
      mime: Some(mime.into()),
    }
  }
}

/// A request under construction. Cheap to rebuild, so retries resend it.
pub struct Req<'a> {
  http: &'a Http,
  method: Method,
  url: String,
  query: Vec<(String, String)>,
  headers: HeaderMap,
  body: Body,
  cookies: bool,
  throttle: bool,
  retries: u32,
  timeout: Option<Duration>,
}

impl<'a> Req<'a> {
  /// GET requests retry twice on network errors and 5xx; others never.
  pub(super) fn new(http: &'a Http, method: Method, url: String) -> Self {
    let retries = if method == Method::GET { 2 } else { 0 };
    Self {
      http,
      method,
      url,
      query: Vec::new(),
      headers: HeaderMap::new(),
      body: Body::Empty,
      cookies: true,
      throttle: true,
      retries,
      timeout: None,
    }
  }

  pub fn query(mut self, key: &str, value: impl ToString) -> Self {
    self.query.push((key.to_owned(), value.to_string()));
    self
  }

  pub fn queries<K: AsRef<str>, V: ToString>(
    mut self,
    pairs: impl IntoIterator<Item = (K, V)>,
  ) -> Self {
    for (k, v) in pairs {
      self.query.push((k.as_ref().to_owned(), v.to_string()));
    }
    self
  }

  pub fn header(mut self, name: &str, value: impl AsRef<str>) -> Self {
    if let (Ok(n), Ok(v)) = (
      HeaderName::from_bytes(name.as_bytes()),
      HeaderValue::from_str(value.as_ref()),
    ) {
      self.headers.insert(n, v);
    }
    self
  }

  /// Compact JSON body.
  pub fn json<T: Serialize + ?Sized>(self, body: &T) -> Self {
    let text = serde_json::to_string(body).expect("serializable body");
    self.json_text(text)
  }

  /// JSON body sent byte-for-byte (when a signature covers the exact text).
  pub fn json_text(mut self, text: impl Into<String>) -> Self {
    self.body = Body::Bytes(
      text.into().into_bytes(),
      "application/json;charset=UTF-8".into(),
    );
    self
  }

  /// `application/x-www-form-urlencoded` body.
  pub fn form<K: AsRef<str>, V: AsRef<str>>(
    mut self,
    pairs: impl IntoIterator<Item = (K, V)>,
  ) -> Self {
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
      ser.append_pair(k.as_ref(), v.as_ref());
    }
    self.body = Body::Bytes(
      ser.finish().into_bytes(),
      "application/x-www-form-urlencoded".into(),
    );
    self
  }

  pub fn bytes(mut self, data: Vec<u8>, content_type: &str) -> Self {
    self.body = Body::Bytes(data, content_type.into());
    self
  }

  pub fn multipart(mut self, parts: Vec<Part>) -> Self {
    self.body = Body::Multipart(parts);
    self
  }

  /// Neither send the platform cookies nor keep the ones the response sets
  /// (CDN, object storage, third-party hosts, logged-out page fetches).
  pub fn no_cookies(mut self) -> Self {
    self.cookies = false;
    self
  }

  /// Skip the per-platform pacing for this request.
  pub fn no_throttle(mut self) -> Self {
    self.throttle = false;
    self
  }

  /// How many times to retry on network errors, 429 and 5xx (GET defaults to 2, others to 0).
  pub fn retries(mut self, n: u32) -> Self {
    self.retries = n;
    self
  }

  /// Timeout of this request instead of the client's (`--timeout`).
  pub fn timeout(mut self, timeout: Duration) -> Self {
    self.timeout = Some(timeout);
    self
  }

  fn full_url(&self) -> Result<String> {
    if self.query.is_empty() {
      return Ok(self.url.clone());
    }
    let mut url = url::Url::parse(&self.url)
      .map_err(|e| Error::internal(format!("bad url {}: {e}", self.url)))?;
    url.query_pairs_mut().extend_pairs(self.query.iter());
    Ok(url.into())
  }

  fn build(&self, url: &str) -> Result<wreq::RequestBuilder> {
    let mut headers = self.http.headers.clone();
    for (k, v) in &self.headers {
      headers.insert(k.clone(), v.clone());
    }
    if self.cookies {
      let cookie = self.http.cookie_header();
      if !cookie.is_empty() {
        headers.insert(
          "cookie",
          HeaderValue::from_str(&cookie).map_err(|e| Error::internal(e.to_string()))?,
        );
      }
    }
    let mut rb = self.http.client.request(self.method.clone(), url);
    if let Some(t) = self.timeout {
      rb = rb.timeout(t);
    }
    match &self.body {
      Body::Empty => {}
      Body::Bytes(data, ct) => {
        if !headers.contains_key("content-type") {
          headers.insert(
            "content-type",
            HeaderValue::from_str(ct).map_err(|e| Error::internal(e.to_string()))?,
          );
        }
        rb = rb.body(data.clone());
      }
      Body::Multipart(parts) => {
        let mut form = wreq::multipart::Form::new();
        for p in parts {
          let mut part = wreq::multipart::Part::bytes(p.data.clone());
          if let Some(f) = &p.file_name {
            part = part.file_name(f.clone());
          }
          if let Some(m) = &p.mime {
            part = part
              .mime_str(m)
              .map_err(|e| Error::internal(e.to_string()))?;
          }
          form = form.part(p.name.clone(), part);
        }
        rb = rb.multipart(form);
      }
    }
    Ok(rb.headers(headers))
  }

  async fn execute(&self) -> Result<wreq::Response> {
    let url = self.full_url()?;
    let mut attempt = 0;
    loop {
      if self.throttle {
        self.http.throttle().await;
      }
      let started = Instant::now();
      let result = self.build(&url)?.send().await;
      self.http.last.set(Some(Instant::now()));
      match result {
        Ok(resp) => {
          tracing::debug!(
            "{} {} -> {} ({} ms)",
            self.method,
            redact(&url),
            resp.status(),
            started.elapsed().as_millis()
          );
          // A request sent without the jar must not write into it either:
          // a site's guest cookies would replace the session's (X's `ct0`).
          if self.cookies {
            self.http.absorb_cookies(&resp);
          }
          let status = resp.status();
          let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
          if retryable && attempt < self.retries {
            attempt += 1;
            backoff(attempt).await;
            continue;
          }
          return Ok(resp);
        }
        Err(e) => {
          tracing::debug!("{} {} failed: {e}", self.method, redact(&url));
          if attempt < self.retries {
            attempt += 1;
            backoff(attempt).await;
            continue;
          }
          return Err(Error::network(format!(
            "{} {}: {e}",
            self.method,
            redact(&url)
          )));
        }
      }
    }
  }

  pub async fn send(self) -> Result<Resp> {
    let resp = self.execute().await?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let url = resp.uri().to_string();
    let body = resp
      .bytes()
      .await
      .map_err(|e| Error::network(e.to_string()))?
      .to_vec();
    Ok(Resp {
      status,
      headers,
      body,
      url,
    })
  }

  /// Send, require a 2xx status and parse the body as JSON.
  pub async fn value(self) -> Result<Value> {
    self.send().await?.check()?.value()
  }

  /// Read a response as it arrives, handing each chunk to `f`, until the
  /// server ends it or `f` returns `false` (server-sent events and other
  /// long-lived feeds; give the request a long `timeout`).
  pub async fn each_chunk(self, mut f: impl FnMut(&[u8]) -> bool) -> Result<()> {
    let resp = self.execute().await?;
    let status = resp.status();
    if !status.is_success() {
      let body = resp
        .bytes()
        .await
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
      return Err(status_error(status, &body));
    }
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
      let chunk = chunk.map_err(|e| Error::network(e.to_string()))?;
      if !f(&chunk) {
        break;
      }
    }
    Ok(())
  }

  /// Stream a successful response's body onto the end of `file`, calling
  /// `progress(bytes)` per chunk; the status and headers come back with the count.
  pub async fn append_to(
    self,
    file: &mut tokio::fs::File,
    mut progress: impl FnMut(u64),
  ) -> Result<Fetched> {
    let resp = self.execute().await?;
    let status = resp.status();
    if !status.is_success() {
      return Err(status_error(status, ""));
    }
    let headers = resp.headers().clone();
    let mut bytes = 0u64;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
      let chunk = chunk.map_err(|e| Error::network(e.to_string()))?;
      file.write_all(&chunk).await?;
      bytes += chunk.len() as u64;
      progress(chunk.len() as u64);
    }
    Ok(Fetched {
      status,
      headers,
      bytes,
    })
  }
}

async fn backoff(attempt: u32) {
  let base = 1000u64 << (attempt - 1).min(4);
  let jitter = rand::rng().random_range(0..1000);
  tokio::time::sleep(Duration::from_millis(base + jitter)).await;
}

/// Drop the query string from logged URLs; it may carry tokens.
/// The URL without its query, and with Telegram Bot API tokens
/// (`/bot<id>:<secret>/`) masked, for logs.
fn redact(url: &str) -> String {
  let url = url.split('?').next().unwrap_or(url);
  let Some(start) = url.find("/bot").map(|i| i + 4) else {
    return url.to_owned();
  };
  let rest = &url[start..];
  let end = rest.find('/').unwrap_or(rest.len());
  let token = &rest[..end];
  match token.split_once(':') {
    Some((id, _)) if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) => {
      format!("{}***{}", &url[..start], &rest[end..])
    }
    _ => url.to_owned(),
  }
}
