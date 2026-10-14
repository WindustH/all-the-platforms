//! Links and ids.
//!
//! Posts are group ids (16–19 digits) behind many link forms: `/article/<id>/`,
//! `/video/<id>/`, `/w/<id>/` (微头条), `/group/<id>/`, `/a<id>/`, the same on
//! `m.toutiao.com`, and `m.toutiao.com/is/<code>/` short links. Users are
//! the `MS4w…` token of `/c/user/token/<token>/`, which the user feed takes;
//! numeric user ids only identify.

use std::sync::LazyLock;

use atp_core::{Error, Result};
use regex::Regex;

use crate::api::{Api, MOBILE, WWW};

static POST_PATH: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"/(?:article|video|w|group|trending)/(\d{6,})|/a(\d{12,})").expect("valid")
});
static USER_TOKEN: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(MS4w[A-Za-z0-9_\-]+)").expect("valid"));

/// What kind of page a post is, as its mobile link names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  Article,
  Video,
  /// 微头条 (a short post).
  Thread,
}

impl Kind {
  pub fn segment(self) -> &'static str {
    match self {
      Kind::Article => "article",
      Kind::Video => "video",
      Kind::Thread => "w",
    }
  }
}

pub struct PostRef {
  pub id: String,
  pub kind: Kind,
}

/// A post from a link, a short link (followed), or a bare id.
pub async fn post(api: &Api, arg: &str) -> Result<PostRef> {
  let arg = arg.trim();
  let link = if arg.contains("/is/") {
    api.ctx.http.final_url(arg).await?
  } else {
    arg.to_owned()
  };
  if let Some(m) = POST_PATH.captures(&link) {
    let id = m
      .get(1)
      .or(m.get(2))
      .map(|g| g.as_str().to_owned())
      .unwrap_or_default();
    let kind = if link.contains("/video/") {
      Kind::Video
    } else if link.contains("/w/") {
      Kind::Thread
    } else {
      Kind::Article
    };
    return Ok(PostRef { id, kind });
  }
  if arg.len() >= 6 && arg.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(PostRef {
      id: arg.to_owned(),
      kind: Kind::Article,
    });
  }
  Err(Error::input(format!(
    "not a Toutiao post: {arg} (expected a toutiao.com link or a group id)"
  )))
}

/// The `MS4w…` token of a user link or bare token.
pub fn user_token(arg: &str) -> Result<String> {
  let arg = arg.trim().trim_start_matches('@');
  match USER_TOKEN.captures(arg) {
    Some(m) => Ok(m[1].to_owned()),
    None => Err(Error::input(format!(
      "not a Toutiao user token: {arg} (expected a toutiao.com/c/user/token/MS4w… link or the MS4w… token)"
    ))),
  }
}

pub fn post_url(id: &str) -> String {
  format!("{WWW}/article/{id}/")
}

pub fn mobile_url(post: &PostRef) -> String {
  format!("{MOBILE}/{}/{}/", post.kind.segment(), post.id)
}

pub fn user_url(token: &str) -> String {
  format!("{WWW}/c/user/token/{token}/")
}
