//! Users: feed authors, the account (`profile/get`, `userInfo`), profiles
//! (`visionProfileReduced`) and users in search results and relation lists.
//! Users are keyed by their eid (`3x…`); the numeric `userId` goes in
//! `extra.user_id`.

use atp_core::{User, UserStats, Value, ValueExt};

use super::put;
use crate::refs::profile_url;

fn base(id: String, name: Option<String>, avatar: Option<String>) -> User {
  User {
    url: Some(profile_url(&id)),
    id,
    name: name.unwrap_or_default(),
    avatar,
    ..User::default()
  }
}

/// A feed or detail author: `{id, name, headerUrl, following, livingInfo}`.
pub fn author(v: &Value) -> Option<User> {
  let mut user = base(
    v.first_str(&["id", "eid", "authorId"])?,
    v.first_str(&["name", "userName"]),
    v.first_str(&["headerUrl", "headurl", "avatar"]),
  );
  user.followed = v.bool("following");
  user.verified = v.bool("verifiedDetail.newVerified").unwrap_or(false);
  put(
    &mut user.extra,
    "living",
    v.bool("livingInfo.living").map(Value::from),
  );
  Some(user)
}

/// The author of a mobile share page photo (`userEid`, `userName`, `headUrl`).
pub fn share_author(photo: &Value) -> Option<User> {
  let mut user = base(
    photo.str("userEid")?,
    photo.str("userName"),
    photo.str("headUrl"),
  );
  user.handle = photo.str("kwaiId");
  put(
    &mut user.extra,
    "user_id",
    photo.str("userId").map(Value::from),
  );
  Some(user)
}

/// The account as `/rest/v/profile/get` describes it.
pub fn me(v: &Value) -> Option<User> {
  let mut user = base(
    v.first_str(&["eid", "userDefineId"])?,
    v.first_str(&["userName", "user_name"]),
    v.first_str(&["userHead", "headurl"]),
  );
  user.bio = v.first_str(&["userText", "user_text"]);
  user.handle = v.str("kwaiId");
  user.stats = UserStats {
    followers: v.first_count(&["fans", "fan"]),
    following: v.first_count(&["follows", "follow"]),
    posts: v.first_count(&["photo", "photoCount"]),
    likes: v.first_count(&["liked", "likedCount"]),
    ..UserStats::default()
  };
  put(&mut user.extra, "user_id", v.str("userId").map(Value::from));
  user.raw = Some(v.clone());
  Some(user)
}

/// The account as the `userInfo` query describes it.
pub fn user_info(v: &Value) -> Option<User> {
  let mut user = base(v.str("eid")?, v.str("name"), v.str("avatar"));
  put(&mut user.extra, "user_id", v.str("userId").map(Value::from));
  user.raw = Some(v.clone());
  Some(user)
}

/// `visionProfileReduced`: name, avatar, bio, gender, follow state.
pub fn profile(v: &Value) -> Option<User> {
  let p = v.at("userProfile.profile");
  let mut user = base(p.str("user_id")?, p.str("user_name"), p.str("headurl"));
  user.bio = p.str("user_text");
  user.followed = v.bool("userProfile.isFollowing");
  put(&mut user.extra, "gender", p.str("gender").map(Value::from));
  put(
    &mut user.extra,
    "living",
    v.bool("userProfile.livingInfo.living").map(Value::from),
  );
  user.raw = Some(v.clone());
  Some(user)
}

/// A user of a search result or relation list.
pub fn listed_user(v: &Value) -> Option<User> {
  let mut user = base(
    v.first_str(&["user_id", "eid", "id", "userId"])?,
    v.first_str(&["user_name", "name", "userName"]),
    v.first_str(&["headurl", "headerUrl", "avatar", "userHead"]),
  );
  user.handle = v.first_str(&["kwaiId", "kwai_id"]);
  user.bio = v.first_str(&["user_text", "description", "userText"]);
  user.followed = v.bool("isFollowing").or_else(|| v.bool("following"));
  user.stats = UserStats {
    followers: v.first_count(&["fansCount", "fans", "fan"]),
    following: v.first_count(&["followCount", "follows"]),
    posts: v.first_count(&["photoCount", "photo"]),
    ..UserStats::default()
  };
  user.raw = Some(v.clone());
  Some(user)
}
