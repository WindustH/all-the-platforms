//! Amap answers → the core model. Amap writes missing values as `[]`, which
//! `ValueExt::str` already reads as absent.

use atp_core::{Collection, Media, Post, Value, ValueExt};

use crate::refs::place_url;

/// A POI of the place APIs (v5) as a post: name as title, address as text.
pub fn poi(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let mut post = Post {
    kind: "poi".into(),
    title: v.str("name"),
    text: address(v),
    url: Some(place_url(&id)),
    media: photos(v),
    raw: Some(v.clone()),
    ..Post::default()
  };
  post.id = id;
  for (key, path) in POI_FIELDS {
    if let Some(value) = v.str(path) {
      post.extra.insert((*key).into(), value.into());
    }
  }
  if let Some(n) = v.u64("distance") {
    post.extra.insert("distance_m".into(), n.into());
  }
  Some(post)
}

/// Extra fields of a POI: (name in `extra`, path in the answer).
const POI_FIELDS: &[(&str, &str)] = &[
  ("location", "location"),
  ("type", "type"),
  ("typecode", "typecode"),
  ("province", "pname"),
  ("city", "cityname"),
  ("district", "adname"),
  ("adcode", "adcode"),
  ("citycode", "citycode"),
  ("parent", "parent"),
  ("tel", "business.tel"),
  ("rating", "business.rating"),
  ("cost", "business.cost"),
  ("business_area", "business.business_area"),
  ("open_today", "business.opentime_today"),
  ("open_week", "business.opentime_week"),
  ("tag", "business.tag"),
  ("alias", "business.alias"),
  ("entrance", "navi.entr_location"),
  ("exit", "navi.exit_location"),
  ("floor", "indoor.truefloor"),
];

/// "province city district address", without repeats (municipalities name
/// the city twice).
fn address(v: &Value) -> Option<String> {
  let mut parts: Vec<String> = Vec::new();
  for path in ["pname", "cityname", "adname", "address"] {
    if let Some(p) = v.str(path)
      && !parts.contains(&p)
    {
      parts.push(p);
    }
  }
  (!parts.is_empty()).then(|| parts.join(" "))
}

/// Photos, sent as an array or as a single object.
fn photos(v: &Value) -> Vec<Media> {
  let one = std::slice::from_ref(v.at("photos"));
  let list = match v.at("photos") {
    Value::Array(a) => a.as_slice(),
    Value::Object(_) => one,
    _ => &[],
  };
  list
    .iter()
    .filter_map(|p| {
      let mut m = Media::image(p.str("url")?);
      m.alt = p.str("title");
      Some(m)
    })
    .collect()
}

/// An input tip (`/v3/assistant/inputtips`): places, bus lines or queries.
pub fn tip(v: &Value) -> Option<Post> {
  let name = v.str("name")?;
  let id = v.str("id");
  let mut post = Post {
    id: id.clone().unwrap_or_else(|| name.clone()),
    kind: "tip".into(),
    title: Some(name),
    text: [v.str("district"), v.str("address")]
      .into_iter()
      .flatten()
      .reduce(|a, b| format!("{a} {b}")),
    url: id.as_deref().map(place_url),
    raw: Some(v.clone()),
    ..Post::default()
  };
  for (key, path) in [
    ("location", "location"),
    ("adcode", "adcode"),
    ("typecode", "typecode"),
  ] {
    if let Some(value) = v.str(path) {
      post.extra.insert(key.into(), value.into());
    }
  }
  Some(post)
}

/// An administrative district (`/v3/config/district`).
pub fn district(v: &Value, parent: Option<&str>) -> Option<Collection> {
  let mut c = Collection {
    id: v.str("adcode")?,
    kind: "district".into(),
    name: v.str("name").unwrap_or_default(),
    description: v.str("level"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  for (key, value) in [
    ("center", v.str("center")),
    ("citycode", v.str("citycode")),
    ("parent", parent.map(str::to_owned)),
  ] {
    if let Some(value) = value {
      c.extra.insert(key.into(), value.into());
    }
  }
  Some(c)
}
