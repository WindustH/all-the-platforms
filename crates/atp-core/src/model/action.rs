//! Results of writes and of the login commands.

use jiff::Timestamp;
use serde::Serialize;

use super::*;

/// Result of a write operation.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Action {
  /// `like`, `unlike`, `follow`, `comment`, `publish`, `delete` ...
  pub action: String,
  pub target: String,
  pub ok: bool,
  /// Id of anything the action created (comment, post ...).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message: Option<String>,
}

impl Action {
  pub fn done(action: impl Into<String>, target: impl Into<String>) -> Self {
    Self {
      action: action.into(),
      target: target.into(),
      ok: true,
      ..Self::default()
    }
  }

  pub fn with_id(mut self, id: impl Into<String>) -> Self {
    self.id = Some(id.into());
    self
  }

  pub fn with_url(mut self, url: impl Into<String>) -> Self {
    self.url = Some(url.into());
    self
  }

  pub fn with_message(mut self, message: impl Into<String>) -> Self {
    self.message = Some(message.into());
    self
  }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AuthStatus {
  pub authenticated: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub user: Option<Box<User>>,
  /// Where the credential came from: `qrcode`, `cookie`, `browser:chrome`, `env` ...
  #[serde(skip_serializing_if = "Option::is_none")]
  pub source: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub saved_at: Option<Timestamp>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message: Option<String>,
}
