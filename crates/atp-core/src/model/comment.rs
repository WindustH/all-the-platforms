//! Comments and their reply threads.

use jiff::Timestamp;
use serde::Serialize;
use serde_json::Value;

use super::*;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Comment {
  pub id: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub author: Option<User>,
  pub text: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub created_at: Option<Timestamp>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub likes: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub reply_count: Option<u64>,
  /// Name of the user this comment answers, inside a thread.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub reply_to: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub location: Option<String>,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub replies: Vec<Comment>,
  /// Where the replies of `comments --replies` stopped early (`--max-pages`):
  /// `replies POST ID --cursor …` continues them.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub replies_cursor: Option<String>,
  #[serde(skip_serializing_if = "Extra::is_empty")]
  pub extra: Extra,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub raw: Option<Value>,
}
