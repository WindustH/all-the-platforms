//! Upstream JSON -> core models.
//!
//! The web player, the desktop client and old endpoints serve the same
//! song in two shapes (`ar` / `al` / `dt` and `artists` / `album` /
//! `duration`); each mapper reads a field from every path it can appear
//! under.

mod music;

pub use music::{album, artist, episode, playlist, podcast, song, with_tracks};

use atp_core::text::from_millis;
use atp_core::{Comment, FileEntry, User, Value, ValueExt, json};

use crate::refs;

/// Plain-http image links -> https.
pub fn https(url: String) -> String {
  match url.strip_prefix("http://") {
    Some(rest) => format!("https://{rest}"),
    None => url,
  }
}

fn time(v: &Value, paths: &[&str]) -> Option<jiff::Timestamp> {
  paths
    .iter()
    .find_map(|p| v.i64(p))
    .filter(|ms| *ms > 0)
    .and_then(from_millis)
}

/// A user from a profile, a follow list, a comment or a search result.
pub fn user(v: &Value) -> Option<User> {
  let id = v.first_str(&["userId", "id"])?;
  let mut u = User {
    url: Some(refs::user_url(&id)),
    name: v.first_str(&["nickname", "name"]).unwrap_or_default(),
    avatar: v.str("avatarUrl").map(https),
    bio: v
      .first_str(&["signature", "description"])
      .map(|s| s.trim().to_owned())
      .filter(|s| !s.is_empty()),
    verified: v.i64("authStatus") == Some(1),
    followed: v.bool("followed"),
    created_at: time(v, &["createTime"]),
    raw: Some(v.clone()),
    id,
    ..User::default()
  };
  u.stats.followers = v.first_count(&["followeds", "followedCount"]);
  u.stats.following = v.first_count(&["follows", "followCount"]);
  u.stats.posts = v.count("playlistCount");
  if v.bool("mutual") == Some(true) {
    u.extra.insert("mutual".into(), json!(true));
  }
  if let Some(vip) = v.i64("vipType").filter(|t| *t > 0) {
    u.extra.insert("vip_type".into(), json!(vip));
  }
  Some(u)
}

/// A user profile (`v1/user/detail`): the profile plus level and plays.
pub fn profile(v: &Value) -> Option<User> {
  let mut u = user(v.at("profile"))?;
  u.created_at = u.created_at.or_else(|| time(v, &["createTime"]));
  if let Some(n) = v.u64("listenSongs") {
    u.stats.other.insert("songs_played".into(), n);
  }
  if let Some(level) = v.i64("level") {
    u.extra.insert("level".into(), json!(level));
  }
  u.raw = Some(v.clone());
  Some(u)
}

/// A comment (or a floor reply) of a thread.
pub fn comment(v: &Value) -> Option<Comment> {
  let id = v.str("commentId")?;
  let replied = v.at("beReplied.0");
  let mut c = Comment {
    id,
    author: user(v.at("user")),
    text: v.first_str(&["content", "richContent"]).unwrap_or_default(),
    created_at: time(v, &["time"]),
    likes: v.count("likedCount"),
    reply_count: v.first_count(&["replyCount", "showFloorComment.replyCount"]),
    reply_to: replied
      .str("beRepliedCommentId")
      .or_else(|| v.str("parentCommentId").filter(|p| p != "0")),
    location: v.str("ipLocation.location"),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  if let Some(name) = replied.str("user.nickname") {
    c.extra.insert("reply_to_user".into(), json!(name));
  }
  attachments(v, &mut c);
  if let Some(thread) = v.str("threadId") {
    c.extra.insert("thread".into(), json!(thread));
  }
  Some(c)
}

/// Pictures, stickers and voice comments (语音评论): pictures and
/// stickers in `extra`, a voice comment only by its length.
fn attachments(v: &Value, c: &mut Comment) {
  let mut label = None;
  if let Some(url) = v.str("contentPicUrl") {
    c.extra.insert("images".into(), json!([https(url)]));
    label = Some("[image]".to_owned());
  }
  if let Some(url) = v.str("expressionUrl") {
    c.extra.insert("sticker".into(), json!(https(url)));
    label = Some("[sticker]".to_owned());
  }
  if let Some(ms) = v.u64("voiceDurationMillSecond").filter(|ms| *ms > 0) {
    let secs = ms.div_ceil(1000);
    c.extra.insert("voice_seconds".into(), json!(secs));
    label = Some(format!("[voice comment, {secs} s]"));
  }
  if let Some(label) = label.filter(|_| c.text.trim().is_empty()) {
    c.text = label;
  }
}

/// A file of your music cloud drive (云盘).
pub fn cloud_file(v: &Value) -> Option<FileEntry> {
  let id = v.first_str(&["songId", "simpleSong.id"])?;
  let mut f = FileEntry {
    name: v
      .first_str(&["fileName", "songName", "simpleSong.name"])
      .unwrap_or_else(|| id.clone()),
    size: v.u64("fileSize"),
    updated_at: time(v, &["addTime"]),
    md5: v.str("md5"),
    raw: Some(v.clone()),
    id,
    ..FileEntry::default()
  };
  for (key, path) in [
    ("song", "songName"),
    ("artist", "artist"),
    ("album", "album"),
    ("bitrate", "bitrate"),
  ] {
    if let Some(value) = v.str(path) {
      f.extra.insert(key.into(), json!(value));
    }
  }
  if let Some(matched) = v.str("simpleSong.id").filter(|s| *s != f.id) {
    f.extra.insert("matched_song".into(), json!(matched));
  }
  Some(f)
}
