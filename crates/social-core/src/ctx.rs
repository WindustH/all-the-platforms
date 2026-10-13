//! What a platform works with at run time: HTTP client, files and session extras.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use crate::error::{Error, Result};
use crate::http::Http;
use crate::platform::{Cap, PlatformInfo};
use crate::store::{RefKind, Session, Store};

/// Runtime handed to a platform: HTTP client (with the session cookies), files and session extras.
pub struct Ctx {
  pub info: PlatformInfo,
  pub http: Http,
  pub store: Store,
  extra: RefCell<BTreeMap<String, String>>,
  extra_changed: Cell<bool>,
}

impl Ctx {
  pub fn new(
    info: PlatformInfo,
    http: Http,
    store: Store,
    extra: BTreeMap<String, String>,
  ) -> Self {
    Self {
      info,
      http,
      store,
      extra: RefCell::new(extra),
      extra_changed: Cell::new(false),
    }
  }

  /// How to log in, for error hints: every way the platform offers.
  pub fn login_hint(&self) -> String {
    let id = self.info.id;
    let cookies = !self.info.cookie_domains.is_empty();
    let mut ways = Vec::new();
    if self.info.supports(Cap::QrLogin) {
      ways.push(format!("`social {id} login` (QR code)"));
    }
    if self.info.supports(Cap::TokenLogin) {
      ways.push(format!("`social {id} login --token '...'`"));
    }
    if cookies && cfg!(feature = "browser") {
      ways.push(format!("`social {id} login --browser`"));
    }
    if cookies {
      ways.push(format!("`social {id} login --cookie '...'`"));
    }
    match ways.split_last() {
      None => format!("see `social guide {id}`"),
      Some((last, [])) => format!("run {last}"),
      Some((last, rest)) => format!("run {} or {last}", rest.join(", ")),
    }
  }

  /// A platform value saved with the session (tokens, device ids ...).
  pub fn extra(&self, key: &str) -> Option<String> {
    self.extra.borrow().get(key).cloned()
  }

  pub fn set_extra(&self, key: &str, value: &str) {
    let mut extra = self.extra.borrow_mut();
    if extra.get(key).map(String::as_str) != Some(value) {
      extra.insert(key.to_owned(), value.to_owned());
      self.extra_changed.set(true);
    }
  }

  pub fn session_changed(&self) -> bool {
    self.http.cookies_changed() || self.extra_changed.get()
  }

  pub fn session(&self) -> Session {
    Session {
      cookies: self.http.cookies(),
      extra: self.extra.borrow().clone(),
      source: None,
      saved_at: None,
    }
  }

  /// Resolve `#N` against the last printed post list.
  pub fn post_ref(&self, arg: &str) -> Result<String> {
    self.store.resolve(RefKind::Post, arg)
  }

  /// Resolve `#N` against the last printed user list.
  pub fn user_ref(&self, arg: &str) -> Result<String> {
    self.store.resolve(RefKind::User, arg)
  }

  /// Resolve `#N` against the last printed list of topics, folders, lists ...
  pub fn collection_ref(&self, arg: &str) -> Result<String> {
    self.store.resolve(RefKind::Collection, arg)
  }

  /// Fail with `not_authenticated` (and a login hint) unless these cookies are present.
  pub fn require_login(&self, required: &[&str]) -> Result<()> {
    match required.iter().find(|c| !self.http.has_cookie(c)) {
      None => Ok(()),
      Some(c) => Err(
        Error::auth(format!("not logged in (missing cookie `{c}`)")).with_hint(self.login_hint()),
      ),
    }
  }
}
