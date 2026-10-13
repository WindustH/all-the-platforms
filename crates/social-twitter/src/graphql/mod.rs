//! GraphQL operations: fallback query ids, feature flags and how each is sent.
//!
//! Query ids rotate with web client releases; the ids here are fallbacks
//! (`web` refreshes them from the live client). Like the web client, queries
//! are GET and mutations POST: sent as POST, some queries (search, followers)
//! treat an expired session as anonymous and answer empty instead of 401.
//! Feature sets follow the reference client: GET sends only the enabled flags
//! (missing ones default to off upstream and long URLs are rejected), POST
//! sends the whole set.

mod features;
mod ops;

use serde_json::{Map, Value};

pub use self::ops::*;

pub type Flags = &'static [(&'static str, bool)];

/// One GraphQL operation.
#[derive(Debug, Clone, Copy)]
pub struct Op {
  pub name: &'static str,
  /// Fallback query id.
  pub id: &'static str,
  pub features: Flags,
  pub toggles: Flags,
  pub post: bool,
  /// Also works with a guest token (no login).
  pub guest: bool,
}

impl Op {
  const fn get(name: &'static str, id: &'static str, features: Flags) -> Self {
    Self {
      name,
      id,
      features,
      toggles: &[],
      post: false,
      guest: false,
    }
  }

  const fn post(name: &'static str, id: &'static str, features: Flags) -> Self {
    Self {
      post: true,
      ..Self::get(name, id, features)
    }
  }

  const fn guest(self) -> Self {
    Self {
      guest: true,
      ..self
    }
  }

  const fn toggles(self, toggles: Flags) -> Self {
    Self { toggles, ..self }
  }

  /// Feature object for the request (`compact`: enabled flags only).
  pub fn features(&self, compact: bool) -> Value {
    flags(self.features, compact)
  }

  pub fn field_toggles(&self) -> Option<Value> {
    (!self.toggles.is_empty()).then(|| flags(self.toggles, false))
  }
}

fn flags(set: Flags, compact: bool) -> Value {
  let map: Map<String, Value> = set
    .iter()
    .filter(|(_, on)| *on || !compact)
    .map(|(k, on)| ((*k).to_owned(), Value::Bool(*on)))
    .collect();
  Value::Object(map)
}
