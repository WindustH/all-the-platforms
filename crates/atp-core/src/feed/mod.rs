//! Web feeds (RSS 0.9x / 1.0 / 2.0, Atom, JSON Feed), for platforms that
//! read them: a feed as [`Feed`] with its entries as posts ([`parse`]),
//! fetched with conditional requests and found from a site's page when
//! given one ([`load`]), and entry references `FEED_URL#ENTRY_ID`
//! ([`entry_ref`], [`split_entry_ref`]).
//!
//! Parsing is feed-rs, which also reads the iTunes, Podcasting 2.0 and
//! Media RSS extensions: enclosures become the entry's media, with their
//! duration; episode and season numbers go into `extra`.

mod fetch;
mod parse;

use jiff::Timestamp;

use crate::ctx::Ctx;
use crate::error::{Error, Result};
use crate::model::Post;
use crate::text::decode_uri_component;

pub use self::fetch::{Got, discover, get};
pub use self::parse::parse;

/// A parsed feed. Its entries are posts of kind `entry` in feed order:
/// `id` is the entry's id (an RSS `guid`, an Atom `id`, or a hash of its
/// link and title), `url` its link.
#[derive(Debug, Clone, Default)]
pub struct Feed {
  /// The feed's address (after redirects and discovery).
  pub url: String,
  /// `rss`, `atom` or `json`.
  pub format: &'static str,
  pub title: Option<String>,
  /// The site the feed belongs to.
  pub link: Option<String>,
  pub description: Option<String>,
  pub author: Option<String>,
  pub image: Option<String>,
  pub language: Option<String>,
  pub categories: Vec<String>,
  pub updated: Option<Timestamp>,
  pub entries: Vec<Post>,
}

impl Feed {
  /// The entry with this id.
  pub fn entry(&self, id: &str) -> Option<&Post> {
    self.entries.iter().find(|entry| entry.id == id)
  }
}

/// Fetch and parse a feed. A web page instead of a feed is searched for
/// the feed it announces (`<link rel="alternate" type="application/rss+xml">`
/// …), which is loaded instead.
pub async fn load(ctx: &Ctx, url: &str) -> Result<Feed> {
  let got = get(ctx, url).await?;
  match parse(&got.body, &got.url) {
    Ok(feed) => Ok(feed),
    Err(e) => {
      let page = String::from_utf8_lossy(&got.body);
      let Some(found) = discover(&page, &got.url).filter(|found| *found != got.url) else {
        return Err(e);
      };
      let got = get(ctx, &found).await?;
      parse(&got.body, &got.url)
    }
  }
}

/// Characters an entry id keeps as they are after `#`; the rest is
/// percent-encoded.
fn fragment_safe(c: char) -> bool {
  c.is_ascii_alphanumeric() || "-._~:/?@!$&'()*+,;=".contains(c)
}

/// `FEED_URL#ENTRY_ID`: a reference to one entry of a feed.
pub fn entry_ref(feed: &str, id: &str) -> String {
  let mut out = format!("{feed}#");
  for c in id.chars() {
    if fragment_safe(c) {
      out.push(c);
    } else {
      let mut buf = [0u8; 4];
      for b in c.encode_utf8(&mut buf).bytes() {
        out.push_str(&format!("%{b:02X}"));
      }
    }
  }
  out
}

/// `(feed URL, entry id)` of an [`entry_ref`]; `None` without a `#…` part.
pub fn split_entry_ref(arg: &str) -> Option<(String, String)> {
  let (feed, id) = arg.trim().split_once('#')?;
  if id.is_empty() || !(feed.starts_with("http://") || feed.starts_with("https://")) {
    return None;
  }
  Some((feed.to_owned(), decode_uri_component(id)))
}

/// The error for a document that is not a feed.
fn not_a_feed(url: &str, detail: impl std::fmt::Display) -> Error {
  Error::input(format!("{url} is not an RSS, Atom or JSON feed ({detail})"))
    .with_hint("give the feed's address, or a page that links to its feed")
}
