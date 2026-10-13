//! LINE payloads → the shared model.

use social_core::{User, Value, ValueExt};

pub fn profile(v: &Value) -> User {
  User {
    id: v.str("userId").unwrap_or_default(),
    name: v.str("displayName").unwrap_or_default(),
    avatar: v.str("pictureUrl"),
    bio: v.str("statusMessage"),
    raw: Some(v.clone()),
    ..User::default()
  }
}
