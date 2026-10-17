//! Posts, accounts and feeds as arguments.
//!
//! - posts: `at://<did>/app.bsky.feed.post/<rkey>`, or a link
//!   `https://bsky.app/profile/<handle or did>/post/<rkey>`;
//! - accounts: `@handle`, `handle`, a DID, a profile link, or `me`;
//! - feeds: `at://<did>/app.bsky.feed.generator/<rkey>` or a link
//!   `https://bsky.app/profile/<handle or did>/feed/<rkey>`;
//! - lists: `at://<did>/app.bsky.graph.list/<rkey>` or a link
//!   `https://bsky.app/profile/<handle or did>/lists/<rkey>`.

use atp_core::{Error, Result};
use url::Url;

pub const POST: &str = "app.bsky.feed.post";
pub const GENERATOR: &str = "app.bsky.feed.generator";
pub const LIST: &str = "app.bsky.graph.list";

/// A record in someone's repository: `at://<repo>/<collection>/<rkey>`,
/// where `repo` may still be a handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtUri {
  pub repo: String,
  pub collection: String,
  pub rkey: String,
}

impl AtUri {
  pub fn parse(uri: &str) -> Option<Self> {
    let rest = uri.trim().strip_prefix("at://")?;
    let mut parts = rest.splitn(3, '/');
    let (repo, collection, rkey) = (parts.next()?, parts.next()?, parts.next()?);
    if repo.is_empty() || collection.is_empty() || rkey.is_empty() || rkey.contains('/') {
      return None;
    }
    Some(Self {
      repo: repo.to_owned(),
      collection: collection.to_owned(),
      rkey: rkey.to_owned(),
    })
  }

  pub fn uri(&self) -> String {
    format!("at://{}/{}/{}", self.repo, self.collection, self.rkey)
  }
}

/// A post, feed or list link on bsky.app (`…/<segment>/<rkey>`) as an
/// [`AtUri`] of `collection`; `what` names it in errors.
fn record(arg: &str, collection: &str, segment: &str, what: &str) -> Result<AtUri> {
  let arg = arg.trim();
  if let Some(uri) = AtUri::parse(arg) {
    if uri.collection != collection {
      return Err(Error::input(format!("not a {what}: {arg}")));
    }
    return Ok(uri);
  }
  let url = Url::parse(arg).map_err(|_| {
    Error::input(format!(
      "not a {what}: {arg} (expected an at:// URI or a bsky.app link)"
    ))
  })?;
  let segments: Vec<&str> = url
    .path_segments()
    .map(Iterator::collect)
    .unwrap_or_default();
  match segments.as_slice() {
    ["profile", repo, kind, rkey, ..] if *kind == segment && !rkey.is_empty() => Ok(AtUri {
      repo: repo.to_string(),
      collection: collection.to_owned(),
      rkey: rkey.to_string(),
    }),
    _ => Err(Error::input(format!("not a {what} link: {arg}"))),
  }
}

pub fn post(arg: &str) -> Result<AtUri> {
  record(arg, POST, "post", "post")
}

pub fn feed(arg: &str) -> Result<AtUri> {
  record(arg, GENERATOR, "feed", "feed")
}

pub fn list(arg: &str) -> Result<AtUri> {
  record(arg, LIST, "lists", "list")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorRef {
  Me,
  /// A handle or a DID, as the API takes actors.
  Actor(String),
}

pub fn actor(arg: &str) -> Result<ActorRef> {
  let arg = arg.trim();
  if arg == "me" {
    return Ok(ActorRef::Me);
  }
  if let Ok(url) = Url::parse(arg)
    && url.scheme().starts_with("http")
  {
    let segments: Vec<&str> = url
      .path_segments()
      .map(Iterator::collect)
      .unwrap_or_default();
    return match segments.as_slice() {
      ["profile", actor, ..] if !actor.is_empty() => Ok(ActorRef::Actor(actor.to_string())),
      _ => Err(Error::input(format!("not a profile link: {arg}"))),
    };
  }
  if let Some(uri) = AtUri::parse(arg) {
    return Ok(ActorRef::Actor(uri.repo));
  }
  let actor = arg.trim_start_matches('@').trim_start_matches("at://");
  if actor.starts_with("did:") {
    return Ok(ActorRef::Actor(actor.to_owned()));
  }
  if actor.contains('.') {
    return Ok(ActorRef::Actor(actor.to_ascii_lowercase()));
  }
  Err(Error::input(format!(
    "not an account: {arg} (expected @handle, a DID, a profile link or me)"
  )))
}

/// The bsky.app link of a post.
pub fn post_url(repo: &str, rkey: &str) -> String {
  format!("https://bsky.app/profile/{repo}/post/{rkey}")
}

pub fn profile_url(actor: &str) -> String {
  format!("https://bsky.app/profile/{actor}")
}

/// The bsky.app link of a list.
pub fn list_url(repo: &str, rkey: &str) -> String {
  format!("https://bsky.app/profile/{repo}/lists/{rkey}")
}
