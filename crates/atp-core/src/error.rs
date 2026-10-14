//! One error type for every platform, carrying a stable machine-readable code.

use std::fmt;

use serde::Serialize;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Stable error codes shared by all platforms (`error.code` in structured output).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
  NotAuthenticated,
  VerificationRequired,
  IpBlocked,
  RateLimited,
  SignatureError,
  InvalidInput,
  NotFound,
  PermissionDenied,
  UnsupportedOperation,
  NetworkError,
  UpstreamError,
  InternalError,
}

impl ErrorCode {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::NotAuthenticated => "not_authenticated",
      Self::VerificationRequired => "verification_required",
      Self::IpBlocked => "ip_blocked",
      Self::RateLimited => "rate_limited",
      Self::SignatureError => "signature_error",
      Self::InvalidInput => "invalid_input",
      Self::NotFound => "not_found",
      Self::PermissionDenied => "permission_denied",
      Self::UnsupportedOperation => "unsupported_operation",
      Self::NetworkError => "network_error",
      Self::UpstreamError => "upstream_error",
      Self::InternalError => "internal_error",
    }
  }

  /// Process exit code: 2 for bad input (like clap), 3 for auth problems, 1 otherwise.
  pub fn exit_code(self) -> u8 {
    match self {
      Self::InvalidInput => 2,
      Self::NotAuthenticated => 3,
      _ => 1,
    }
  }
}

#[derive(Debug, Clone, Serialize)]
pub struct Error {
  pub code: ErrorCode,
  pub message: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub hint: Option<String>,
  /// A page that clears the error in a browser: a captcha, a risk check.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  /// A human check that passing it in the person's browser cannot clear
  /// (see [`Error::unclearable`]).
  #[serde(skip)]
  pub unclearable: bool,
}

impl Error {
  pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
    Self {
      code,
      message: message.into(),
      hint: None,
      url: None,
      unclearable: false,
    }
  }

  pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
    self.hint = Some(hint.into());
    self
  }

  pub fn with_url(mut self, url: impl Into<String>) -> Self {
    self.url = Some(url.into());
    self
  }

  /// A check a browser pass cannot clear: the request carried no cookies,
  /// the challenge is bound to the browser's fingerprint, or the site wants
  /// device headers ATP does not make. The CLI then prints the error with
  /// its page instead of opening it and retrying.
  pub fn unclearable(mut self) -> Self {
    self.unclearable = true;
    self
  }

  /// The page to open for this error: `url`, else the first link in the
  /// hint or the message.
  pub fn link(&self) -> Option<String> {
    self.url.clone().or_else(|| {
      [self.hint.as_deref(), Some(self.message.as_str())]
        .into_iter()
        .flatten()
        .find_map(first_link)
    })
  }

  pub fn auth(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::NotAuthenticated, message)
  }

  /// A captcha or risk check (`verification_required`); add its page with
  /// [`Error::with_url`].
  pub fn verification(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::VerificationRequired, message)
  }

  pub fn input(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::InvalidInput, message)
  }

  pub fn not_found(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::NotFound, message)
  }

  pub fn upstream(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::UpstreamError, message)
  }

  pub fn network(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::NetworkError, message)
  }

  pub fn internal(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::InternalError, message)
  }

  pub fn unsupported(operation: impl AsRef<str>) -> Self {
    let operation = operation.as_ref();
    Self::new(
      ErrorCode::UnsupportedOperation,
      format!("`{operation}` is not supported on this platform"),
    )
  }
}

/// The first `http(s)://` link in `text`, without trailing punctuation.
fn first_link(text: &str) -> Option<String> {
  let start = text.find("https://").or_else(|| text.find("http://"))?;
  let link: String = text[start..]
    .chars()
    .take_while(|c| !c.is_whitespace() && !"\"'`<>()（）「」，。".contains(*c))
    .collect();
  Some(
    link
      .trim_end_matches(['.', ',', ';', ':', '!', '?'])
      .to_owned(),
  )
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.message)
  }
}

impl std::error::Error for Error {}

impl From<wreq::Error> for Error {
  fn from(e: wreq::Error) -> Self {
    Self::network(e.to_string())
  }
}

impl From<serde_json::Error> for Error {
  fn from(e: serde_json::Error) -> Self {
    Self::upstream(format!("unexpected response: {e}"))
  }
}

impl From<std::io::Error> for Error {
  fn from(e: std::io::Error) -> Self {
    Self::internal(e.to_string())
  }
}
