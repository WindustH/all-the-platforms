//! Collections: topics, folders, lists, channels and chats.

use serde::Serialize;
use serde_json::Value;

use super::*;

impl Collection {
  /// What other commands should receive to refer back to this collection:
  /// folder options take ids, not links.
  pub fn reference(&self) -> &str {
    &self.id
  }
}

/// A named group of content: topic / hashtag, favorites folder, list.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Collection {
  pub id: String,
  /// `topic`, `folder`, `list` ...
  pub kind: String,
  pub name: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub items: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub followers: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub views: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub owner: Option<User>,
  #[serde(skip_serializing_if = "Extra::is_empty")]
  pub extra: Extra,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub raw: Option<Value>,
}
