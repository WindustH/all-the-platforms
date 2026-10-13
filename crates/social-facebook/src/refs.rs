//! Profiles, posts and groups as arguments.
//!
//! Profiles: a handle (`nasa`, `@nasa`), a numeric id, or a link. Posts: a
//! permalink (`…/posts/…`, `permalink.php?story_fbid=…&id=…`, photos,
//! videos, reels, `fb.watch`, `/share/p/…`) or `PROFILE/POST_ID`; Facebook has
//! no route for a bare post id. Groups: an id, a slug or a link.

use social_core::{Error, Result};
use url::Url;

use crate::web::{HOME, canonical, is_facebook};

pub fn profile_url(arg: &str) -> String {
  let arg = arg.trim().trim_start_matches('@');
  if let Some(url) = facebook_url(arg) {
    return url;
  }
  if arg.bytes().all(|b| b.is_ascii_digit()) {
    format!("{HOME}/profile.php?id={arg}")
  } else {
    format!("{HOME}/{arg}")
  }
}

/// The page of one profile tab (`about`, `photos`, `videos` ...).
pub fn tab_url(arg: &str, tab: &str) -> String {
  let arg = arg.trim().trim_start_matches('@');
  if arg.bytes().all(|b| b.is_ascii_digit()) {
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
    return Ok(if owner.bytes().all(|b| b.is_ascii_digit()) {
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
  (!path.is_empty()
    && !path.contains('/')
    && path != "profile.php"
    && !path.bytes().all(|b| b.is_ascii_digit()))
  .then(|| path.to_owned())
}
