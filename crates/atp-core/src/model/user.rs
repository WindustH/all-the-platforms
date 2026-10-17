//! Users and their counters.

use jiff::Timestamp;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

use super::*;

/// Counters on a user.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UserStats {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub followers: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub following: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub posts: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub likes: Option<u64>,
  #[serde(flatten)]
  pub other: BTreeMap<String, u64>,
}

impl UserStats {
  pub fn is_empty(&self) -> bool {
    self.followers.is_none()
      && self.following.is_none()
      && self.posts.is_none()
      && self.likes.is_none()
      && self.other.is_empty()
  }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct User {
  pub id: String,
  pub name: String,
  /// @screen_name, url_token, red id ... whatever the platform shows as a handle.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub handle: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub avatar: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub bio: Option<String>,
  #[serde(skip_serializing_if = "is_false")]
  pub verified: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub location: Option<String>,
  #[serde(skip_serializing_if = "UserStats::is_empty")]
  pub stats: UserStats,
  /// Whether the logged-in account follows this user, when known.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub followed: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub created_at: Option<Timestamp>,
  /// Platform fields. Rows of the logged-in account's own `following` /
  /// `followers` use these keys for the relation, where the platform
  /// tells it: `mutual` (bool: you follow each other), `followed_at`
  /// (when the follow began), `groups` (ids of the follow groups the user
  /// is in), `special` (bool: a special follow) and `remark` (the name
  /// you gave them).
  #[serde(skip_serializing_if = "Extra::is_empty")]
  pub extra: Extra,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub raw: Option<Value>,
}

impl User {
  /// What other commands should receive to refer back to this user.
  pub fn reference(&self) -> &str {
    self.url.as_deref().unwrap_or(&self.id)
  }
}
