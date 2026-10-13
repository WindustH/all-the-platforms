//! Users: profiles, list entries and authors.

use super::*;

/// A user as items and comments embed them (camelCase or snake_case).
pub fn user_brief(v: &Value) -> User {
  let handle = v.first_str(&["uniqueId", "unique_id"]);
  User {
    id: v.first_str(&["id", "uid", "user_id"]).unwrap_or_default(),
    name: v
      .str("nickname")
      .or_else(|| handle.clone())
      .unwrap_or_default(),
    url: handle.as_deref().map(user_url),
    avatar: v.first_str(&[
      "avatarLarger",
      "avatarMedium",
      "avatarThumb",
      "avatar_larger.url_list.0",
      "avatar_medium.url_list.0",
      "avatar_thumb.url_list.0",
    ]),
    bio: v.str("signature").filter(|s| !s.trim().is_empty()),
    verified: v.bool("verified").unwrap_or(false) || v.i64("custom_verify").is_some_and(|n| n > 0),
    extra: sec_uid_extra(v),
    raw: Some(v.clone()),
    handle,
    ..User::default()
  }
}

fn sec_uid_extra(v: &Value) -> atp_core::Extra {
  let mut extra = atp_core::Extra::new();
  if let Some(s) = v.first_str(&["secUid", "sec_uid"]) {
    extra.insert("sec_uid".into(), s.into());
  }
  if let Some(true) = v.bool("privateAccount") {
    extra.insert("private".into(), true.into());
  }
  if let Some(r) = v.str("region") {
    extra.insert("region".into(), r.into());
  }
  extra
}

/// A profile: `userInfo` of a page or of `/api/user/detail/`.
pub fn user(info: &Value) -> User {
  let u = info.at("user");
  let stat = |key: &str| stat(info, "statsV2", "stats", key);
  let mut stats = UserStats {
    followers: stat("followerCount"),
    following: stat("followingCount"),
    posts: stat("videoCount"),
    likes: stat("heartCount").or_else(|| stat("heart")),
    ..UserStats::default()
  };
  if let Some(n) = stat("friendCount") {
    stats.other.insert("friends".into(), n);
  }
  if let Some(n) = stat("diggCount") {
    stats.other.insert("liked".into(), n);
  }
  let mut out = user_brief(u);
  out.stats = stats;
  out.created_at = u.i64("createTime").filter(|t| *t > 0).and_then(from_secs);
  // relation: 0 none, 1 following, 2 friends (mutual).
  out.followed = u.i64("relation").map(|r| r == 1 || r == 2);
  for (key, path) in [
    ("language", "language"),
    ("bio_link", "bioLink.link"),
    ("commerce", "commerceUserInfo.commerceUser"),
    ("seller", "ttSeller"),
  ] {
    if let Some(v) = Some(u.at(path)).filter(|v| !v.is_null()) {
      out.extra.insert(key.into(), v.clone());
    }
  }
  out.raw = Some(info.clone());
  out
}

/// A user of a follower list or a user search (`user` + `stats`, either case).
pub fn listed_user(v: &Value) -> User {
  let info = if v.at("user_info").is_object() {
    v.at("user_info")
  } else {
    v
  };
  if info.at("user").is_object() {
    return user(info);
  }
  let mut out = user_brief(info);
  out.stats = UserStats {
    followers: info.count("follower_count"),
    following: info.count("following_count"),
    posts: info.count("aweme_count"),
    likes: info.count("total_favorited"),
    ..UserStats::default()
  };
  out
}
