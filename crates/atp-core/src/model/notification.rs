//! Notifications.

use jiff::Timestamp;
use serde::Serialize;
use serde_json::Value;

use super::*;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Notification {
  pub id: String,
  /// `like`, `comment`, `mention`, `follow`, `reply`, `system` ...
  pub kind: String,
  pub text: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub actor: Option<User>,
  /// Title or excerpt of the content the notification is about.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub target: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub created_at: Option<Timestamp>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub unread: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub raw: Option<Value>,
}
