//! Feeds and entries as arguments.
//!
//! - feeds: a URL (`https://blog.rust-lang.org/feed.xml`, or a site whose
//!   page announces its feed), a host and path without the scheme, or an
//!   RSSHub route `rsshub:/github/trending/daily` (resolved on the
//!   instance in use);
//! - entries: `FEED_URL#ENTRY_ID`, `miniflux:<id>`, or an entry's link
//!   from an earlier list.

use atp_core::{Error, Result, feed};

/// What an entry argument names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
  /// An entry of a feed fetched by ATP.
  Feed { feed: String, id: String },
  /// An entry of the Miniflux account.
  Miniflux(u64),
}

pub const HUB_SCHEME: &str = "rsshub:";
pub const MINIFLUX_SCHEME: &str = "miniflux:";

/// A feed argument as a URL.
pub fn feed(arg: &str, hub: &str) -> Result<String> {
  let arg = arg.trim();
  if let Some(route) = arg.strip_prefix(HUB_SCHEME) {
    let route = route.trim_start_matches('/');
    if route.is_empty() {
      return Err(Error::input(
        "an RSSHub route looks like rsshub:/github/trending/daily",
      ));
    }
    return Ok(format!("{hub}/{route}"));
  }
  if arg.starts_with("http://") || arg.starts_with("https://") {
    return Ok(arg.to_owned());
  }
  let looks_like_host = arg.split('/').next().is_some_and(|host| host.contains('.'));
  if looks_like_host && !arg.contains(char::is_whitespace) {
    return Ok(format!("https://{arg}"));
  }
  Err(Error::input(format!(
    "not a feed: {arg} (expected a URL or rsshub:/route)"
  )))
}

/// An entry argument, if it is one in its own right (not a link to look up).
pub fn entry(arg: &str) -> Option<Entry> {
  let arg = arg.trim();
  if let Some(id) = arg.strip_prefix(MINIFLUX_SCHEME) {
    return id.parse().ok().map(Entry::Miniflux);
  }
  feed::split_entry_ref(arg).map(|(feed, id)| Entry::Feed { feed, id })
}

/// A Miniflux entry: `miniflux:<id>` or the bare id.
pub fn miniflux_id(arg: &str) -> Option<u64> {
  let arg = arg.trim();
  arg
    .strip_prefix(MINIFLUX_SCHEME)
    .unwrap_or(arg)
    .parse()
    .ok()
}
