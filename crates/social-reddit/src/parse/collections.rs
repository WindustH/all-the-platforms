//! `t5` subreddits and multireddits as collections.

use social_core::text::from_secs;
use social_core::{Collection, Value, ValueExt};

use super::{flag, put, time};
use crate::api::WWW;
use crate::refs::sub_url;

/// A `t5` subreddit as a collection (`kind` is `user` for profile subreddits).
pub fn subreddit(d: &Value) -> Option<Collection> {
  let name = d.str("display_name")?;
  let is_user = d.str("subreddit_type").as_deref() == Some("user");
  let mut c = Collection {
    kind: if is_user { "user" } else { "subreddit" }.into(),
    name: d
      .str("display_name_prefixed")
      .unwrap_or_else(|| name.clone()),
    description: d
      .first_str(&["public_description", "title"])
      .map(|t| t.trim().to_owned()),
    url: Some(
      d.str("url")
        .map(|u| format!("{WWW}{u}"))
        .unwrap_or_else(|| sub_url(&name)),
    ),
    followers: d.count("subscribers"),
    raw: Some(d.clone()),
    id: name,
    ..Collection::default()
  };
  let x = &mut c.extra;
  put(x, "title", d.str("title").map(Value::from));
  put(x, "fullname", d.str("name").map(Value::from));
  put(
    x,
    "active",
    d.first_count(&["active_user_count", "accounts_active"])
      .map(Value::from),
  );
  put(
    x,
    "created_at",
    time(d, "created_utc").map(|t| t.to_string().into()),
  );
  put(
    x,
    "icon",
    d.first_str(&["community_icon", "icon_img"])
      .map(|u| u.split('?').next().unwrap_or_default().to_owned())
      .filter(|u| !u.is_empty())
      .map(Value::from),
  );
  put(x, "nsfw", flag(d, "over18"));
  put(x, "subscribed", flag(d, "user_is_subscriber"));
  Some(c)
}

pub fn multireddit(thing: &Value) -> Option<Collection> {
  let d = thing.at("data");
  let path = d.str("path")?;
  let subs: Vec<Value> = d
    .list("subreddits")
    .iter()
    .filter_map(|s| s.str("name"))
    .map(Value::from)
    .collect();
  let owner = d.str("owner").map(|o| super::named(&o, None));
  let mut c = Collection {
    id: path.trim_end_matches('/').to_owned(),
    kind: "multireddit".into(),
    name: d.first_str(&["display_name", "name"]).unwrap_or_default(),
    description: d.str("description_md").filter(|s| !s.is_empty()),
    url: Some(format!("{WWW}{path}")),
    items: Some(subs.len() as u64),
    followers: d.u64("num_subscribers"),
    owner,
    ..Collection::default()
  };
  c.extra.insert("subreddits".into(), Value::from(subs));
  if let Some(v) = d.str("visibility") {
    c.extra.insert("visibility".into(), v.into());
  }
  if let Some(t) = d.i64("created_utc").and_then(from_secs) {
    c.extra.insert("created_at".into(), t.to_string().into());
  }
  Some(c)
}
