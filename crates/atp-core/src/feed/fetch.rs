//! Fetching feeds: conditional requests (`If-None-Match` /
//! `If-Modified-Since`) against copies kept in the platform's private
//! cache, and the feed a web page announces.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::ctx::Ctx;
use crate::error::{Error, Result};
use crate::http::{Resp, TOOL_USER_AGENT, status_error};

const ACCEPT: &str = "application/rss+xml, application/atom+xml, application/feed+json, \
application/xml;q=0.9, text/xml;q=0.9, application/json;q=0.8, */*;q=0.7";
/// How long a fetched feed and its validators are kept for the next request.
const KEEP: Duration = Duration::from_secs(30 * 86_400);
/// `type`s of `<link rel="alternate">` that announce a feed.
const FEED_TYPES: &[&str] = &[
  "application/rss+xml",
  "application/atom+xml",
  "application/feed+json",
  "application/json",
];

/// The validators of the cached copy of one feed.
#[derive(Serialize, Deserialize)]
struct Validators {
  etag: Option<String>,
  modified: Option<String>,
  /// Where the copy came from, after redirects.
  url: String,
}

/// A fetched document and its final address.
pub struct Got {
  pub body: Vec<u8>,
  pub url: String,
  /// The server answered `304 Not Modified`: `body` is the cached copy.
  pub cached: bool,
}

fn key(url: &str) -> String {
  let digest = Sha1::digest(url.as_bytes());
  let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
  format!("feed-{}", &hex[..20])
}

/// GET a feed as a feed reader does: a declared client name, and the
/// validators of the last copy, so an unchanged feed answers `304` and the
/// copy is used.
pub async fn get(ctx: &Ctx, url: &str) -> Result<Got> {
  let key = key(url);
  let body_name = format!("{key}.body");
  let meta: Option<Validators> = ctx.store.cache_get(&key, KEEP);
  let copy = meta
    .as_ref()
    .and_then(|_| ctx.store.cache_get_bytes(&body_name, KEEP));
  let mut req = ctx
    .http
    .get(url)
    .no_cookies()
    .header("user-agent", TOOL_USER_AGENT)
    .header("accept", ACCEPT)
    .retries(1);
  if let (Some(meta), Some(_)) = (&meta, &copy) {
    if let Some(etag) = &meta.etag {
      req = req.header("if-none-match", etag);
    }
    if let Some(modified) = &meta.modified {
      req = req.header("if-modified-since", modified);
    }
  }
  let resp = req.send().await?;
  if resp.status.as_u16() == 304
    && let (Some(meta), Some(body)) = (meta, copy)
  {
    return Ok(Got {
      body,
      url: meta.url,
      cached: true,
    });
  }
  if !resp.status.is_success() {
    return Err(refused(&resp));
  }
  let validators = Validators {
    etag: resp.header("etag").map(str::to_owned),
    modified: resp.header("last-modified").map(str::to_owned),
    url: resp.url.clone(),
  };
  if validators.etag.is_some() || validators.modified.is_some() {
    ctx.store.cache_put_bytes(&body_name, &resp.body);
    ctx.store.cache_put(&key, &validators);
  }
  Ok(Got {
    body: resp.body,
    url: resp.url,
    cached: false,
  })
}

/// A failed fetch; a browser check (Cloudflare's "Just a moment…") says so
/// plainly, since no feed reader passes one.
fn refused(resp: &Resp) -> Error {
  let text = resp.text();
  let challenge = matches!(resp.status.as_u16(), 403 | 503)
    && (text.contains("Just a moment")
      || text.contains("cf-chl")
      || text.contains("challenge-platform"));
  if !challenge {
    return status_error(resp.status, &text);
  }
  let host = url::Url::parse(&resp.url)
    .ok()
    .and_then(|url| url.host_str().map(str::to_owned))
    .unwrap_or_default();
  // Feeds go out without cookies, and the check is tied to the browser
  // that passes it: a browser pass cannot clear it for ATP.
  Error::verification(format!(
    "{host} answered with a browser check (Cloudflare), which feed readers cannot pass"
  ))
  .with_url(&resp.url)
  .with_hint("the feed is closed to readers; look for another address of it")
  .unclearable()
}

/// The first feed a web page announces with `<link rel="alternate"
/// type="application/rss+xml" href="…">` (or Atom / JSON Feed), as an
/// absolute URL.
pub fn discover(page: &str, base: &str) -> Option<String> {
  let lower = page.to_ascii_lowercase();
  let base = url::Url::parse(base).ok()?;
  let mut at = 0;
  while let Some(found) = lower[at..].find("<link") {
    let start = at + found;
    let end = start + lower[start..].find('>')?;
    at = end;
    let tag = &page[start..end];
    let rel = attr(tag, "rel").unwrap_or_default().to_ascii_lowercase();
    let kind = attr(tag, "type").unwrap_or_default().to_ascii_lowercase();
    let alternate = rel.split_whitespace().any(|r| r == "alternate");
    if alternate
      && FEED_TYPES.iter().any(|t| kind.starts_with(t))
      && let Some(href) = attr(tag, "href")
    {
      return base.join(&href).ok().map(String::from);
    }
  }
  None
}

/// The value of attribute `name` in an HTML tag (quoted or not).
fn attr(tag: &str, name: &str) -> Option<String> {
  let lower = tag.to_ascii_lowercase();
  let pattern = format!("{name}=");
  let mut from = 0;
  while let Some(found) = lower[from..].find(&pattern) {
    let at = from + found;
    from = at + pattern.len();
    if at == 0 || !lower.as_bytes()[at - 1].is_ascii_whitespace() {
      continue;
    }
    let rest = &tag[from..];
    let value = match rest.chars().next()? {
      quote @ ('"' | '\'') => rest[1..].split(quote).next()?,
      _ => rest.split(char::is_whitespace).next()?,
    };
    return Some(value.replace("&amp;", "&"));
  }
  None
}
