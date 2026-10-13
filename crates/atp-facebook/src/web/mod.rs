//! facebook.com as a browser sees it: page fetches (the server checks for a
//! navigation's `Sec-Fetch-*` headers, not the user agent) and replays of the
//! GraphQL operations a page ran ([`replay`]).
//!
//! Logged out, most reads work from the first page; spending a cursor
//! (timelines, more comments) is refused until a session (`c_user` + `xs`)
//! is present. A signed-out page that does not exist still answers 200, so
//! "not found" is decided by which operations came back.

mod replay;

use std::collections::BTreeMap;

use atp_core::{Ctx, Error, ErrorCode, Result, Value};

pub use self::replay::advance;
use crate::api::Api;
use crate::refs::{HOME, canonical};
use crate::relay::{self, Preload, Tokens};

/// A fetched Comet page with what it shipped.
pub struct Page {
  pub url: String,
  pub final_url: String,
  pub status: u16,
  pub html: String,
  pub docs: BTreeMap<String, Value>,
  pub preloads: Vec<Preload>,
  pub tokens: Tokens,
  pub head: BTreeMap<String, String>,
}

impl Page {
  pub fn doc(&self, op: &str) -> Option<&Value> {
    self.docs.get(op)
  }

  pub fn preload(&self, op: &str) -> Option<&Preload> {
    self.preloads.iter().find(|p| p.op == op)
  }

  /// The first paged operation whose name contains one of `parts`.
  pub fn paged(&self, parts: &[&str]) -> Option<&Preload> {
    self
      .preloads
      .iter()
      .find(|p| p.has_cursor() && parts.iter().any(|w| p.op.contains(w)))
  }

  /// A login wall, Facebook's security interstitial, or none of `wanted`.
  pub fn check(&self, what: &str, wanted: &[&str]) -> Result<()> {
    if self.html.contains("blocked by our security systems") {
      return Err(
        Error::new(
          ErrorCode::VerificationRequired,
          format!("Facebook blocked the request for {what}"),
        )
        .with_hint("wait a while, or log in (`atp facebook login --browser`)"),
      );
    }
    if self.login_wall() {
      return Err(
        Error::auth(format!("Facebook shows {what} only to logged-in visitors"))
          .with_hint("log in with `atp facebook login --browser` (or `--cookie 'c_user=…; xs=…'`)"),
      );
    }
    if wanted.is_empty() || wanted.iter().any(|op| self.docs.contains_key(*op)) {
      return Ok(());
    }
    Err(Error::not_found(format!(
      "{what} not found ({})",
      self.missing_reason()
    )))
  }

  fn login_wall(&self) -> bool {
    let wall = self.final_url.contains("/login") || self.final_url.contains("/checkpoint");
    let only_login = !self.docs.is_empty()
      && self
        .docs
        .keys()
        .all(|op| op == "useCometLogInFormQuery" || op == "CometHomeRootQuery");
    wall || only_login
  }

  /// Why the page did not carry what was asked for.
  fn missing_reason(&self) -> String {
    if self.final_url.trim_end_matches('/') == HOME || self.final_url.contains("content_not_found")
    {
      format!("the page redirected to {}", self.final_url)
    } else if self.status >= 400 {
      format!("HTTP {}", self.status)
    } else if self.docs.is_empty() {
      "the page carried no data".into()
    } else {
      format!(
        "the page shipped {} but not what is needed",
        self.docs.keys().cloned().collect::<Vec<_>>().join(", ")
      )
    }
  }
}

/// Fetch a facebook.com page the way a navigation does.
pub async fn fetch(ctx: &Ctx, url: &str) -> Result<Page> {
  let url = canonical(url);
  let resp = ctx
    .http
    .get(&url)
    .header(
      "accept",
      "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    )
    .header("accept-language", "en-US,en;q=0.9")
    .header("sec-fetch-dest", "document")
    .header("sec-fetch-mode", "navigate")
    .header("sec-fetch-site", "none")
    .header("upgrade-insecure-requests", "1")
    .send()
    .await?;
  let html = resp.text();
  let blocks = relay::blocks(&html);
  Ok(Page {
    url,
    final_url: resp.url.clone(),
    status: resp.status.as_u16(),
    docs: relay::documents(&blocks),
    preloads: relay::preloads(&blocks),
    tokens: relay::tokens(&html),
    head: head(&html),
    html,
  })
}

/// The end of a list Facebook continues only for logged-in visitors.
pub fn stop<T>(api: &Api, what: &str) -> atp_core::Page<T> {
  if !api.has_session() {
    atp_core::output::note(&format!(
      "Facebook continues {what} only for logged-in visitors (`atp facebook login --browser`)"
    ));
  }
  atp_core::Page::new(Vec::new(), None)
}

/// The `<meta property|name content>` pairs of the head.
fn head(html: &str) -> BTreeMap<String, String> {
  let end = html.find("</head>").unwrap_or(html.len().min(200_000));
  let head = &html[..end];
  let mut out = BTreeMap::new();
  let mut at = 0;
  while let Some(offset) = head[at..].find("<meta ") {
    let start = at + offset;
    let Some(len) = head[start..].find('>') else {
      break;
    };
    let tag = &head[start..start + len];
    at = start + len;
    let key = attr(tag, "property").or_else(|| attr(tag, "name"));
    if let (Some(key), Some(value)) = (key, attr(tag, "content")) {
      out.entry(key).or_insert(value);
    }
  }
  out
}

fn attr(tag: &str, name: &str) -> Option<String> {
  let start = tag.find(&format!("{name}=\""))? + name.len() + 2;
  let rest = &tag[start..];
  Some(entities(&rest[..rest.find('"')?]))
}

fn entities(s: &str) -> String {
  s.replace("&quot;", "\"")
    .replace("&#039;", "'")
    .replace("&#x27;", "'")
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&amp;", "&")
}
