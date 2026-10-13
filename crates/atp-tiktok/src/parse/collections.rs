//! Hashtags, sounds and favorite folders as collections.

use super::*;

/// A hashtag: `challengeInfo` (`challenge` + `stats`) or a bare challenge.
pub fn hashtag(info: &Value) -> Option<Collection> {
  let c = if info.at("challenge").is_object() {
    info.at("challenge")
  } else {
    info
  };
  let title = c.first_str(&["title", "cha_name"])?;
  let stat = |key: &str| {
    info
      .count(&format!("statsV2.{key}"))
      .or_else(|| info.count(&format!("stats.{key}")))
      .or_else(|| c.count(&format!("stats.{key}")))
  };
  Some(Collection {
    id: c.first_str(&["id", "cid"]).unwrap_or_default(),
    kind: "hashtag".into(),
    url: Some(format!(
      "{}/tag/{}",
      crate::api::WWW,
      crate::api::encode(&title)
    )),
    description: c.first_str(&["desc"]).filter(|d| !d.is_empty()),
    items: stat("videoCount"),
    views: stat("viewCount"),
    name: title,
    raw: Some(info.clone()),
    ..Collection::default()
  })
}

/// A sound: `musicInfo` (`music` + `stats`) or a bare music object.
pub fn sound(info: &Value) -> Option<Collection> {
  let m = if info.at("music").is_object() {
    info.at("music")
  } else {
    info
  };
  let id = m.str("id")?;
  let title = m.str("title").unwrap_or_default();
  let mut extra = atp_core::Extra::new();
  for (key, path) in [
    ("author", "authorName"),
    ("original", "original"),
    ("duration", "duration"),
    ("play_url", "playUrl"),
  ] {
    let v = m.at(path);
    if !v.is_null() {
      extra.insert(key.into(), v.clone());
    }
  }
  Some(Collection {
    url: Some(format!("{}/music/{}-{id}", crate::api::WWW, slug(&title))),
    kind: "sound".into(),
    name: title,
    items: info
      .count("stats.videoCount")
      .or_else(|| m.count("userCount")),
    extra,
    raw: Some(info.clone()),
    id,
    ..Collection::default()
  })
}

/// A playlist (mix) or a favorites collection.
pub fn folder(v: &Value, kind: &str) -> Option<Collection> {
  let v = if v.at("mixInfo").is_object() {
    v.at("mixInfo")
  } else {
    v
  };
  let id = v.first_str(&["id", "mixId", "collectionId"])?;
  let name = v
    .first_str(&["name", "mixName", "collectionName"])
    .unwrap_or_default();
  let owner = v.at("creator");
  Some(Collection {
    url: owner
      .first_str(&["uniqueId", "unique_id"])
      .map(|h| format!("{}/@{h}/{kind}/{}-{id}", crate::api::WWW, slug(&name))),
    kind: kind.into(),
    items: v.count("videoCount").or_else(|| v.count("total")),
    owner: (!owner.is_null()).then(|| user_brief(owner)),
    name,
    raw: Some(v.clone()),
    id,
    ..Collection::default()
  })
}

fn slug(title: &str) -> String {
  let s: String = title
    .chars()
    .map(|c| if c.is_alphanumeric() { c } else { '-' })
    .collect();
  let s = s.trim_matches('-');
  if s.is_empty() { "x".into() } else { s.into() }
}
