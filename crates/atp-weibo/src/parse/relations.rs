//! Rows of relation lists and of the lists that manage them: your own
//! following and followers with what Weibo tells about each relation,
//! follow groups (关注分组), the users you block and favorite tags.

use atp_core::{Collection, User, Value, ValueExt};

use super::user;

/// Which of your own lists a row comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Own {
  Following,
  Followers,
}

/// A user of one of your own lists: on whom you follow, `extra.mutual`
/// (`follow_me`), `extra.special` (特别关注, `special_follow`) and
/// `extra.remark`; on your followers, `extra.mutual` (`following`).
pub fn relation(v: &Value, own: Own) -> Option<User> {
  let mut u = user(v)?;
  let extra = &mut u.extra;
  let mutual = match own {
    Own::Following => v.bool("follow_me"),
    Own::Followers => v.bool("following"),
  };
  if let Some(mutual) = mutual {
    extra.insert("mutual".into(), mutual.into());
  }
  if own == Own::Following {
    if let Some(special) = v.bool("special_follow") {
      extra.insert("special".into(), special.into());
    }
    if let Some(remark) = v.str("remark").filter(|r| !r.is_empty()) {
      extra.insert("remark".into(), remark.into());
    }
  }
  Some(u)
}

/// A follow group of `getGroups`: `mode` is `public`, `private` or
/// `system` (groups Weibo keeps, which cannot be renamed or deleted).
pub fn group(v: &Value) -> Option<Collection> {
  let id = v.first_str(&["idstr", "id"])?;
  let mut c = Collection {
    id,
    kind: "group".into(),
    name: v.str("name").unwrap_or_default(),
    items: v.count("member_count"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(mode) = v.str("mode").filter(|m| !m.is_empty()) {
    c.extra.insert("mode".into(), mode.into());
  }
  Some(c)
}

/// A row of the blocked-users settings page (`getFilteredUsers`): a card
/// with the name (`title_sub`), the picture and what is blocked (`desc1`,
/// in `extra.blocking`). The card names no user id, so the name stands
/// in for it.
pub fn blocked(v: &Value) -> Option<User> {
  let name = v.str("title_sub").filter(|n| !n.is_empty())?;
  let mut u = User {
    id: v
      .first_str(&["uid", "user.idstr"])
      .unwrap_or_else(|| name.clone()),
    url: Some(format!("https://weibo.com/n/{name}")),
    avatar: v.str("pic"),
    name,
    raw: Some(v.clone()),
    ..User::default()
  };
  if let Some(what) = v.str("desc1").filter(|d| !d.is_empty()) {
    u.extra.insert("blocking".into(), what.into());
  }
  Some(u)
}

/// A favorite tag (收藏标签) as a folder.
pub fn tag(v: &Value) -> Option<Collection> {
  let id = v.first_str(&["id", "tag_id"]).filter(|id| id != "0")?;
  Some(Collection {
    id,
    kind: "folder".into(),
    name: v.first_str(&["tag", "name"]).unwrap_or_default(),
    items: v.count("count"),
    raw: Some(v.clone()),
    ..Collection::default()
  })
}

/// The names of the tags a favorite is under (`tags` of a favorites row:
/// objects with `id` and `tag`, as the tag dialog reads them).
pub fn tag_names(v: &Value) -> Vec<String> {
  v.list("tags").iter().filter_map(|t| t.str("tag")).collect()
}
