//! Upstream JSON -> core models.
//!
//! The gateway, the search and the `c.y.qq.com` pages name the same field
//! differently (`encrypt_uin`, `EncryptUin`, `EncUin`; `dissname`, `title`,
//! `diss_name`); each mapper reads a field from every name it comes under.

mod music;

pub use music::{album, artist, playlist, song, toplist};

use atp_core::text::{date_only, from_secs};
use atp_core::{Comment, User, Value, ValueExt, json};

use crate::refs;

/// Plain-http image links -> https.
pub fn https(url: String) -> String {
  match url.strip_prefix("http://") {
    Some(rest) => format!("https://{rest}"),
    None => url,
  }
}

/// A time from Unix seconds or a `YYYY-MM-DD` date under one of `paths`.
fn time(v: &Value, paths: &[&str]) -> Option<jiff::Timestamp> {
  paths.iter().find_map(|p| match v.at(p) {
    Value::String(s) if s.contains('-') => date(s),
    _ => v.i64(p).and_then(from_secs),
  })
}

/// `2003-07-31` as a date without a time of day.
fn date(text: &str) -> Option<jiff::Timestamp> {
  let day = text.get(..10)?.parse::<jiff::civil::Date>().ok()?;
  date_only(day)
}

/// An account number, when the payload has one.
fn number(v: &Value, paths: &[&str]) -> Option<u64> {
  paths.iter().find_map(|p| v.u64(p)).filter(|n| *n > 10_000)
}

/// A user from a search result, a comment, a playlist's creator, a
/// profile, a follow list or a created-playlists page.
pub fn user(v: &Value) -> Option<User> {
  let id = v.first_str(&[
    "encrypt_uin",
    "EncryptUin",
    "EncUin",
    "EncryptedUin",
    "euin",
  ])?;
  let mut u = User {
    url: Some(refs::user_url(&id)),
    name: v
      .first_str(&[
        "nick", "Nick", "name", "Name", "nickname", "hostname", "title",
      ])
      .unwrap_or_default(),
    avatar: v
      .first_str(&[
        "headpic",
        "headurl",
        "avatarUrl",
        "Avatar",
        "AvatarUrl",
        "pic",
        "avatar",
      ])
      .map(https),
    bio: v
      .first_str(&["Desc", "desc", "signature", "Signature"])
      .map(|s| s.trim().to_owned())
      .filter(|s| !s.is_empty()),
    followed: v
      .bool("IsFollow")
      .or_else(|| v.i64("concern_status").map(|n| n == 1)),
    raw: Some(v.clone()),
    id,
    ..User::default()
  };
  u.stats.followers = v.first_count(&["fans_num", "FanNum", "fansnum", "nums.fansnum"]);
  u.stats.following = v.first_count(&["follownum", "nums.follownum", "FollowNum"]);
  u.stats.posts = v.first_count(&["diss_num", "totoal", "nums.dissnum"]);
  if let Some(uin) = number(v, &["musicid", "qq", "creator_uin", "uin"]) {
    u.extra.insert("uin".into(), json!(uin));
  }
  if let Some(identity) = v.first_str(&["identify_title", "IdentityText"]) {
    u.verified = true;
    u.extra.insert("identity".into(), json!(identity));
  }
  Some(u)
}

/// The author of a comment.
fn author(v: &Value) -> Option<User> {
  let id = v.str("EncryptUin")?;
  Some(User {
    url: Some(refs::user_url(&id)),
    name: v.str("Nick").unwrap_or_default(),
    avatar: v.str("Avatar").map(https),
    id,
    ..User::default()
  })
}

/// A comment (or a reply) of a song, an album or a playlist.
pub fn comment(v: &Value) -> Option<Comment> {
  let id = v.str("CmId")?;
  let replied = v.at("RepliedComments.0");
  let mut c = Comment {
    id,
    author: author(v),
    text: v.str("Content").unwrap_or_default(),
    created_at: v.i64("PubTime").and_then(from_secs),
    likes: v.count("PraiseNum"),
    reply_count: v.count("ReplyCnt"),
    reply_to: replied.str("CmId"),
    location: v.str("Location"),
    // The first replies come inline; their JSON is in this comment's raw.
    replies: v
      .list("SubComments")
      .iter()
      .filter_map(|r| comment(r).map(|reply| Comment { raw: None, ..reply }))
      .collect(),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  if let Some(name) = replied.str("Nick") {
    c.extra.insert("reply_to_user".into(), json!(name));
  }
  if let Some(pic) = v.str("Pic").filter(|p| p.starts_with("http")) {
    c.extra.insert("images".into(), json!([https(pic)]));
    if c.text.trim().is_empty() {
      c.text = "[image]".into();
    }
  }
  Some(c)
}
