//! Profiles, posts, groups and Graph ids as arguments, and facebook.com
//! links as one canonical form.
//!
//! Profiles: a handle (`nasa`, `@nasa`), a numeric id, or a link. Posts: a
//! permalink (`…/posts/…`, `permalink.php?story_fbid=…&id=…`, photos,
//! videos, reels, `fb.watch`, `/share/p/…`) or `PROFILE/POST_ID`; Facebook has
//! no route for a bare post id. Groups: an id, a slug or a link. Page posts
//! and comments of the Graph API go by their Graph id (`PAGE_POST`).

use atp_core::{Error, Result};
use url::Url;

pub const HOME: &str = "https://www.facebook.com";

/// Query parameters facebook.com adds per render or for tracking.
const TRACKING: [&str; 15] = [
  "__tn__",
  "fref",
  "mibextid",
  "rdid",
  "ref",
  "refsrc",
  "hc_ref",
  "share_url",
  "eav",
  "paipv",
  "_rdc",
  "_rdr",
  "sfnsn",
  "notif_id",
  "notif_t",
];

pub fn profile_url(arg: &str) -> String {
  let arg = arg.trim().trim_start_matches('@');
  if let Some(url) = facebook_url(arg) {
    return url;
  }
  if is_numeric(arg) {
    format!("{HOME}/profile.php?id={arg}")
  } else {
    format!("{HOME}/{arg}")
  }
}

/// The page of one profile tab (`about`, `photos`, `friends` ...).
pub fn tab_url(arg: &str, tab: &str) -> String {
  let arg = arg.trim().trim_start_matches('@');
  if is_numeric(arg) {
    return format!("{HOME}/profile.php?id={arg}&sk={tab}");
  }
  let base = profile_url(arg);
  match Url::parse(&base) {
    Ok(u) if u.path() == "/profile.php" => format!("{base}&sk={tab}"),
    _ => format!("{}/{tab}", base.trim_end_matches('/')),
  }
}

pub fn post_url(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if let Some(url) = facebook_url(arg) {
    return Ok(url);
  }
  if let Ok(u) = Url::parse(arg)
    && u.host_str() == Some("fb.watch")
  {
    return Ok(arg.to_owned());
  }
  if let Some((owner, id)) = arg.split_once('/')
    && !owner.is_empty()
    && !id.is_empty()
    && !id.contains('/')
  {
    let owner = owner.trim_start_matches('@');
    return Ok(if is_numeric(owner) {
      format!("{HOME}/permalink.php?story_fbid={id}&id={owner}")
    } else {
      format!("{HOME}/{owner}/posts/{id}")
    });
  }
  Err(
    Error::input(format!("not a Facebook post: {arg}")).with_hint(
      "pass the post's link, or PROFILE/POST_ID; Facebook has no route for a bare post id",
    ),
  )
}

pub fn group_url(arg: &str) -> String {
  let arg = arg.trim();
  if let Some(url) = facebook_url(arg) {
    return url;
  }
  format!("{HOME}/groups/{}/", arg.trim_matches('/'))
}

/// A facebook.com link, canonicalized; `None` for anything else.
fn facebook_url(arg: &str) -> Option<String> {
  let with_scheme = if arg.starts_with("http") {
    arg.to_owned()
  } else if arg.contains("facebook.com/") {
    format!("https://{arg}")
  } else {
    return None;
  };
  let u = Url::parse(&with_scheme).ok()?;
  is_facebook(&u.host_str()?.to_ascii_lowercase()).then(|| canonical(&with_scheme))
}

/// The vanity handle in a profile link, if it uses one.
pub fn handle_of(url: &str) -> Option<String> {
  let u = Url::parse(url).ok()?;
  let path = u.path().trim_matches('/');
  (!path.is_empty() && !path.contains('/') && path != "profile.php" && !is_numeric(path))
    .then(|| path.to_owned())
}

fn is_numeric(s: &str) -> bool {
  s.bytes().all(|b| b.is_ascii_digit())
}

// ── links ──────────────────────────────────────────────────────────────────

/// facebook.com links on `www`, https, without per-render tracking.
pub fn canonical(raw: &str) -> String {
  let Ok(mut u) = Url::parse(raw) else {
    return raw.to_owned();
  };
  let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
  if !is_facebook(&host) {
    return raw.to_owned();
  }
  let _ = u.set_host(Some("www.facebook.com"));
  let _ = u.set_scheme("https");
  let kept: Vec<(String, String)> = u
    .query_pairs()
    .filter(|(k, _)| !TRACKING.contains(&k.as_ref()) && !k.starts_with("__cft__"))
    .map(|(k, v)| (k.into_owned(), v.into_owned()))
    .collect();
  if kept.is_empty() {
    u.set_query(None);
  } else {
    u.query_pairs_mut().clear().extend_pairs(kept);
  }
  u.set_fragment(None);
  u.into()
}

pub fn is_facebook(host: &str) -> bool {
  matches!(
    host,
    "www.facebook.com"
      | "web.facebook.com"
      | "m.facebook.com"
      | "mbasic.facebook.com"
      | "facebook.com"
      | "free.facebook.com"
      | "d.facebook.com"
  )
}

/// The destination behind an `l.facebook.com` redirect.
pub fn unshim(raw: &str) -> String {
  let Ok(u) = Url::parse(raw) else {
    return raw.to_owned();
  };
  let host = u.host_str().unwrap_or_default();
  if !matches!(
    host,
    "l.facebook.com" | "lm.facebook.com" | "l.messenger.com"
  ) {
    return raw.to_owned();
  }
  u.query_pairs()
    .find(|(k, _)| k == "u")
    .map(|(_, v)| v.into_owned())
    .unwrap_or_else(|| raw.to_owned())
}

// ── Graph ids ──────────────────────────────────────────────────────────────

/// A Graph API post or comment id (`123_456`) rather than a link or handle.
pub fn is_graph_id(arg: &str) -> bool {
  arg.contains('_') && arg.bytes().all(|b| b.is_ascii_digit() || b == b'_')
}

/// A Graph object id from an argument: `123_456` / `123` as is.
pub fn graph_id(arg: &str) -> Result<String> {
  let a = arg.trim();
  if !a.is_empty() && a.bytes().all(|b| b.is_ascii_digit() || b == b'_') {
    return Ok(a.to_owned());
  }
  Err(
    Error::input(format!("`{arg}` is not a Graph API id"))
      .with_hint("Page posts and comments are written by Graph id (PAGE_POST); `atp facebook page-posts` lists them"),
  )
}

/// The Page part of a Graph post or comment id (`PAGE_POST`, `PAGE_POST_COMMENT`).
pub fn owner_of(id: &str) -> Option<&str> {
  id.split_once('_').map(|(page, _)| page)
}
