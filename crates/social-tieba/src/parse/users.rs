//! Users of profiles, listings and floors.

use super::*;

pub fn user(v: &Value) -> Option<User> {
  let portrait = v
    .first_str(&["portrait", "encry_uid", "portraith"])
    .map(|p| clean_portrait(&p))
    .filter(|p| !p.is_empty());
  let id = v.first_str(&["id", "user_id", "uid"]).filter(|i| i != "0");
  if id.is_none() && portrait.is_none() {
    return None;
  }
  let shown = v.first_str(&[
    "name_show",
    "show_nickname",
    "user_nickname",
    "nickname",
    "name",
    "user_name",
  ]);
  // Hidden accounts show `贴吧用户_xxx` with the bare `贴吧用户` as name.
  let handle = v
    .first_str(&["name", "user_name"])
    .filter(|n| shown.as_ref().is_none_or(|s| !s.starts_with(n.as_str())));
  Some(User {
    id: id.unwrap_or_default(),
    name: shown.clone().or_else(|| handle.clone()).unwrap_or_default(),
    handle,
    url: portrait.as_deref().map(user_url),
    avatar: portrait.as_deref().map(avatar_url),
    bio: v.str("intro"),
    location: v.str("ip_address"),
    stats: user_stats(v),
    followed: v
      .first_str(&["has_concerned", "is_followed", "is_like"])
      .map(|s| s == "1" || s == "true"),
    extra: user_extra(v),
    raw: Some(v.clone()),
    ..User::default()
  })
}

fn user_stats(v: &Value) -> UserStats {
  let mut stats = UserStats {
    followers: count(v, &["fans_num"]),
    following: count(v, &["concern_num"]),
    posts: count(v, &["post_num"]),
    likes: count(v, &["total_agree_num"]),
    ..UserStats::default()
  };
  if let Some(n) = count(v, &["thread_num"]).filter(|n| *n > 0) {
    stats.other.insert("threads".into(), n);
  }
  if let Some(n) = count(v, &["my_like_num", "like_forum_num"]).filter(|n| *n > 0) {
    stats.other.insert("forums".into(), n);
  }
  stats
}

/// Level, Tieba age and moderator role, where set.
fn user_extra(v: &Value) -> social_core::Extra {
  let mut extra = social_core::Extra::new();
  for (key, path) in [
    ("level", "level_id"),
    ("level_name", "level_name"),
    ("tieba_age", "tb_age"),
    ("tieba_uid", "tieba_uid"),
    ("bawu", "bawu_type"),
  ] {
    if let Some(s) = v.str(path).filter(|s| s != "0") {
      extra.insert(key.into(), json!(s));
    }
  }
  extra
}

/// The author of an item: its own object, or the listing's user by id.
pub(super) fn author(item: &Value, users: &Users) -> Option<User> {
  ["author", "user", "replyer"]
    .iter()
    .find_map(|k| user(item.at(k)))
    .or_else(|| {
      let id = item.first_str(&["author_id", "user_id"])?;
      user(users.get(&id)?)
    })
}
