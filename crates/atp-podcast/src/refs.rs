//! Shows and episodes as arguments.
//!
//! - shows: an Apple Podcasts link (`https://podcasts.apple.com/…/id<n>`),
//!   `id<n>` or the number; or the show's feed URL;
//! - episodes: `FEED_URL#GUID` (what episode lists link to), or an Apple
//!   Podcasts episode link (`…/id<show>?i=<episode>`).

use atp_core::{Error, Result, feed};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Show {
  Apple(u64),
  Feed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Episode {
  Apple { show: u64, id: u64 },
  Feed { feed: String, guid: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
  Show(Show),
  Episode(Episode),
}

/// `id123` or `123`.
fn apple_id(text: &str) -> Option<u64> {
  let digits = text.strip_prefix("id").unwrap_or(text);
  (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    .then(|| digits.parse().ok())
    .flatten()
}

fn is_apple(url: &Url) -> bool {
  url
    .host_str()
    .is_some_and(|host| host == "apple.com" || host.ends_with(".apple.com"))
}

pub fn item(arg: &str) -> Result<Item> {
  let arg = arg.trim();
  if let Some(id) = apple_id(arg) {
    return Ok(Item::Show(Show::Apple(id)));
  }
  let url = Url::parse(arg).map_err(|_| bad(arg))?;
  if !matches!(url.scheme(), "http" | "https") {
    return Err(bad(arg));
  }
  if is_apple(&url) {
    let show = url
      .path_segments()
      .into_iter()
      .flatten()
      .find_map(|segment| segment.strip_prefix("id").and_then(apple_id))
      .ok_or_else(|| bad(arg))?;
    let episode = url
      .query_pairs()
      .find(|(key, _)| key == "i")
      .and_then(|(_, id)| id.parse().ok());
    return Ok(match episode {
      Some(id) => Item::Episode(Episode::Apple { show, id }),
      None => Item::Show(Show::Apple(show)),
    });
  }
  if let Some((feed, guid)) = feed::split_entry_ref(arg) {
    return Ok(Item::Episode(Episode::Feed { feed, guid }));
  }
  Ok(Item::Show(Show::Feed(arg.to_owned())))
}

pub fn show(arg: &str) -> Result<Show> {
  match item(arg)? {
    Item::Show(show) => Ok(show),
    Item::Episode(_) => Err(Error::input(format!(
      "{arg} is an episode; give its show (an Apple Podcasts link, id or feed URL)"
    ))),
  }
}

fn bad(arg: &str) -> Error {
  Error::input(format!(
    "not a podcast: {arg} (expected an Apple Podcasts link or id, a feed URL, or FEED_URL#GUID)"
  ))
}

/// The Apple Podcasts page of a show.
pub fn apple_show_url(id: u64) -> String {
  format!("{}/podcast/id{id}", crate::api::APPLE)
}

/// An Apple link without its tracking query (`?uo=4`), keeping `?i=`.
pub fn clean_apple_url(link: &str) -> String {
  let Ok(mut url) = Url::parse(link) else {
    return link.to_owned();
  };
  let episode = url
    .query_pairs()
    .find(|(key, _)| key == "i")
    .map(|(_, id)| id.into_owned());
  url.set_query(None);
  if let Some(id) = episode {
    url.query_pairs_mut().append_pair("i", &id);
  }
  url.to_string()
}
