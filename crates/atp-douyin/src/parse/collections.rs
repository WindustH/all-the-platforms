//! Mixes (合集), short dramas (短剧) and saved music as Collections, and
//! the answers of the calls that unsave or unlike several at once.

use atp_core::{Action, Collection, Error, Extra, Value, ValueExt};

use super::user;
use crate::api::WWW;

/// A mix (`mix_info`): its episodes so far in `items`, the author as
/// `owner`.
pub fn mix(m: &Value) -> Option<Collection> {
  let id = m.first_str(&["mix_id_str", "mix_id"])?;
  Some(Collection {
    url: Some(format!("{WWW}/collection/{id}")),
    kind: "mix".into(),
    name: m.str("mix_name").unwrap_or_default(),
    description: m.str("desc").filter(|d| !d.is_empty()),
    items: m.count("statis.updated_to_episode"),
    views: m.count("statis.play_vv"),
    owner: user(m.at("author")),
    raw: Some(m.clone()),
    id,
    ..Collection::default()
  })
}

/// A short drama (`series_info`), as [`mix`].
pub fn series(s: &Value) -> Option<Collection> {
  let id = s.first_str(&["series_id_str", "series_id"])?;
  Some(Collection {
    kind: "drama".into(),
    name: s.str("series_name").unwrap_or_default(),
    description: s.str("desc").filter(|d| !d.is_empty()),
    items: s.count("stats.updated_to_episode"),
    views: s.count("stats.play_vv"),
    owner: user(s.at("author")),
    raw: Some(s.clone()),
    id,
    ..Collection::default()
  })
}

/// A saved music: its author in `description`, `extra.duration` in
/// seconds and `extra.uses` (posts made with it).
pub fn music(m: &Value) -> Option<Collection> {
  let id = m.first_str(&["id_str", "mid", "id"])?;
  let mut extra = Extra::new();
  if let Some(seconds) = m.f64("duration") {
    extra.insert("duration".into(), seconds.into());
  }
  if let Some(uses) = m.count("user_count") {
    extra.insert("uses".into(), uses.into());
  }
  Some(Collection {
    url: Some(format!("{WWW}/music/{id}")),
    kind: "music".into(),
    name: m.str("title").unwrap_or_default(),
    description: m.first_str(&["author", "owner_nickname"]),
    extra,
    raw: Some(m.clone()),
    id,
    ..Collection::default()
  })
}

/// An Action per id of a call on several (`aweme/cancel/collect/multi`
/// and its kin): the ids it names in `fatal_ids` failed.
pub fn cancelled(
  v: &Value,
  action: &str,
  ids: &[String],
  url: impl Fn(&str) -> Option<String>,
) -> Vec<Action> {
  let fatal: Vec<String> = v
    .list("fatal_ids")
    .iter()
    .filter_map(|id| match id {
      Value::String(s) => Some(s.clone()),
      Value::Number(n) => Some(n.to_string()),
      _ => None,
    })
    .collect();
  let kept = Error::upstream(format!("Douyin did not {action} this one"));
  ids
    .iter()
    .map(|id| {
      if fatal.contains(id) {
        return Action::failed(action, id, &kept);
      }
      let done = Action::done(action, id);
      match url(id) {
        Some(link) => done.with_url(link),
        None => done,
      }
    })
    .collect()
}
