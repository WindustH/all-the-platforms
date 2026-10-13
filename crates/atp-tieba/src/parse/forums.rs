//! Forums of info pages, listings and search.

use super::*;

/// A forum. Its name is the id: Tieba addresses forums by name (`/f?kw=`)
/// and has no lookup by number, so `#N` refs must carry the name; the
/// number is `extra.fid`.
pub fn forum(v: &Value) -> Option<Collection> {
  let name = v.first_str(&["name", "forum_name", "fname", "forum_name_show"])?;
  let mut forum = Collection {
    id: name.clone(),
    kind: "forum".into(),
    url: Some(forum_url(&name)),
    name,
    description: v.first_str(&["slogan", "intro"]),
    items: count(v, &["thread_num"]),
    followers: count(
      v,
      &[
        "member_num",
        "concern_num_ori",
        "concern_num",
        "member_count",
      ],
    ),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(fid) = v.first_str(&["id", "forum_id", "fid"]) {
    forum.extra.insert("fid".into(), json!(fid));
  }
  if let Some(n) = count(v, &["post_num_ori", "post_num"]) {
    forum.extra.insert("posts".into(), json!(n));
  }
  for (key, path) in [
    ("avatar", "avatar"),
    ("category", "first_class"),
    ("subcategory", "second_class"),
    ("level", "level_id"),
    ("level_name", "level_name"),
    ("score", "cur_score"),
  ] {
    if let Some(s) = v.str(path).filter(|s| s != "0") {
      forum.extra.insert(key.into(), json!(s));
    }
  }
  if let Some(liked) = v.first_str(&["is_like", "has_concerned"]) {
    forum.extra.insert("followed".into(), json!(liked == "1"));
  }
  Some(forum)
}
