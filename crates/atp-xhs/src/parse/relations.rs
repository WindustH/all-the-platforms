//! Rows of relation lists: whom a user follows and who follows them, as
//! the profile's relation panel and its search list them, and the message
//! page's list of everyone you follow.

use atp_core::{User, Value, ValueExt};

use super::user_brief;
use crate::refs::user_url;

/// How the logged-in account and a row's user relate, as the web reads
/// it: `follow_status` as a number (1 both, 2 you follow them, 3 they
/// follow you, 4 yourself) or `follow_status` / `fstatus` as that word.
fn relation(v: &Value) -> Option<&'static str> {
  let status = v.first_str(&["follow_status", "fstatus"])?;
  Some(match status.as_str() {
    "1" | "both" => "both",
    "2" | "follows" => "follows",
    "3" | "fans" => "fans",
    "4" | "same" => "same",
    _ => "none",
  })
}

/// A row of the relation panel (`user/followings/self`, its batch
/// manager's `batch_list`, `user/followings`, `user/followers`) or of its
/// search. Rows of your own lists (`own`) carry `extra.mutual` and
/// `extra.remark`.
pub fn relation_user(v: &Value, own: bool) -> Option<User> {
  let mut user = user_brief(v)?;
  let relation = relation(v);
  user.bio = v.str("desc");
  user.verified = ["account_verification_type", "red_official_verify_type"]
    .iter()
    .any(|k| v.i64(k).is_some_and(|t| t > 0))
    || v.bool("show_red_official_verify_icon") == Some(true);
  user.stats.followers = v.first_count(&["fan_count", "fans", "fans_total"]);
  user.stats.posts = v.first_count(&["note_count", "ndiscovery", "discovery_total"]);
  user.stats.likes = v.first_count(&["like_count", "likes"]);
  user.followed = relation.map(|r| matches!(r, "follows" | "both"));
  if own {
    if let Some(r) = relation {
      user.extra.insert("mutual".into(), Value::from(r == "both"));
    }
    if let Some(remark) = v.str("remark") {
      user.extra.insert("remark".into(), Value::from(remark));
    }
  }
  user.raw = Some(v.clone());
  Some(user)
}

/// A row of the message page's list of everyone you follow
/// (`users/following/all`); `status` 1 marks a mutual follow, as the
/// share dialog reads it.
pub fn contact(v: &Value) -> Option<User> {
  let id = v.str("user_id")?;
  let mut user = User {
    url: Some(user_url(&id)),
    name: v.str("nick_name").unwrap_or_default(),
    avatar: v.str("avatar"),
    verified: v.i64("verify_type").is_some_and(|t| t > 0),
    followed: Some(true),
    id,
    ..User::default()
  };
  if let Some(status) = v.i64("status") {
    user.extra.insert("mutual".into(), Value::from(status == 1));
  }
  Some(user)
}
