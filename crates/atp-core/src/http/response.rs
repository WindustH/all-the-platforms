//! Responses: read whole or appended to a file, and status mapping.

use serde::de::DeserializeOwned;
use serde_json::Value;
use wreq::header::HeaderMap;

use super::StatusCode;
use crate::error::{Error, ErrorCode, Result};

/// What [`Req::append_to`] wrote.
pub struct Fetched {
  pub status: StatusCode,
  pub headers: HeaderMap,
  pub bytes: u64,
}

impl Fetched {
  /// Size of the whole file from `Content-Range: bytes a-b/total`.
  pub fn range_total(&self) -> Option<u64> {
    let v = self.headers.get("content-range")?.to_str().ok()?;
    v.rsplit('/').next()?.parse().ok()
  }
}

/// A fully read response.
pub struct Resp {
  pub status: StatusCode,
  pub headers: HeaderMap,
  pub body: Vec<u8>,
  /// Final URL after redirects.
  pub url: String,
}

impl Resp {
  pub fn header(&self, name: &str) -> Option<&str> {
    self.headers.get(name).and_then(|v| v.to_str().ok())
  }

  pub fn text(&self) -> String {
    String::from_utf8_lossy(&self.body).into_owned()
  }

  pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
    serde_json::from_slice(&self.body).map_err(|e| {
      Error::upstream(format!(
        "unexpected response ({e}): {}",
        crate::text::truncate(&self.text(), 200)
      ))
    })
  }

  pub fn value(&self) -> Result<Value> {
    self.json()
  }

  /// Map common failure statuses to error codes; 2xx passes through.
  pub fn check(self) -> Result<Self> {
    if self.status.is_success() {
      Ok(self)
    } else {
      Err(status_error(self.status, &self.text()))
    }
  }
}

pub fn status_error(status: StatusCode, body: &str) -> Error {
  let snippet = crate::text::truncate(&crate::text::one_line(body), 200);
  let code = match status.as_u16() {
    401 => ErrorCode::NotAuthenticated,
    403 => ErrorCode::PermissionDenied,
    404 => ErrorCode::NotFound,
    429 => ErrorCode::RateLimited,
    _ => ErrorCode::UpstreamError,
  };
  Error::new(code, format!("HTTP {status}: {snippet}"))
}
