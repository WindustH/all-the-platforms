//! Dianping pages → core model. Pages carry their data as JSON in
//! scripts: the mobile shop page in `window.__xhrCache__` (the answers of
//! the calls it made on the server, keyed by method, URL and query), the
//! discovery feed in `window.__dx_dump__`, a note's page in
//! `__xhrCache__` too. Search results and member pages are read from their
//! HTML.

mod lists;
mod note;
mod shop;

use atp_core::{Value, ValueExt};

pub use lists::{feed, member, search};
pub use note::note;
pub use shop::shop;

/// The JSON object assigned to `name` in a page (`name = {…}`; indexed
/// assignments such as `name['k'] = …` are skipped).
pub fn assigned(html: &str, name: &str) -> Option<Value> {
  let mut rest = html;
  while let Some(at) = rest.find(name) {
    rest = &rest[at + name.len()..];
    let Some(value) = rest.trim_start().strip_prefix('=') else {
      continue;
    };
    let mut values = serde_json::Deserializer::from_str(value.trim_start()).into_iter::<Value>();
    if let Some(Ok(v)) = values.next() {
      return Some(v);
    }
  }
  None
}

/// The answer the page cached for the call whose key contains `path`.
pub fn cached<'a>(cache: &'a Value, path: &str) -> Option<&'a Value> {
  cache
    .as_object()?
    .iter()
    .find(|(key, _)| key.contains(path))
    .map(|(_, v)| v.at("data"))
}

/// The first capture of `re` in `text`, trimmed and unescaped.
fn capture(re: &regex::Regex, text: &str) -> Option<String> {
  re.captures(text)
    .map(|m| unescape(m[1].trim()))
    .filter(|s| !s.is_empty())
}

/// HTML entities the pages use in text.
fn unescape(text: &str) -> String {
  text
    .replace("&amp;", "&")
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&nbsp;", " ")
}
