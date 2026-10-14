//! Amap answers → the core model. Amap writes missing values as `[]`, which
//! `ValueExt::str` already reads as absent; its points are GCJ-02
//! `lng,lat` strings.

use atp_core::geo::{Coord, Place, System};
use atp_core::{Collection, Media, Post, Value, ValueExt, json};

use crate::refs::place_url;

/// A GCJ-02 point from Amap's `lng,lat`.
pub fn coord(text: &str) -> Option<Coord> {
  Coord::parse(text, System::Gcj02)
}

/// The point at `path` as JSON, for `extra` and values.
pub fn coord_at(v: &Value, path: &str) -> Option<Value> {
  let c = coord(&v.str(path)?)?;
  Some(json!(c))
}

/// A POI of the place APIs (v5) as a post: the name as title, the rest in
/// `place`.
pub fn poi(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let place = Place {
    location: v.str("location").as_deref().and_then(coord),
    address: v.str("address"),
    category: v.str("type"),
    phone: v.str("business.tel"),
    rating: v.f64("business.rating"),
    distance_m: v.u64("distance"),
    province: v.str("pname"),
    city: v.str("cityname"),
    district: v.str("adname"),
    adcode: v.str("adcode"),
    hours: v.str("business.opentime_today"),
    ..Place::default()
  };
  let mut post = Post {
    kind: "poi".into(),
    title: v.str("name"),
    url: Some(place_url(&id)),
    place: Some(Box::new(place)),
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
  for (key, path) in [
    ("entrance", "navi.entr_location"),
    ("exit", "navi.exit_location"),
  ] {
    if let Some(point) = coord_at(v, path) {
      post.extra.insert(key.into(), point);
    }
  }
  Some(post)
}

/// Extra fields of a POI: (name in `extra`, path in the answer).
const POI_FIELDS: &[(&str, &str)] = &[
  ("typecode", "typecode"),
  ("citycode", "citycode"),
  ("parent", "parent"),
  ("cost", "business.cost"),
  ("business_area", "business.business_area"),
  ("open_week", "business.opentime_week"),
  ("tag", "business.tag"),
  ("alias", "business.alias"),
  ("floor", "indoor.truefloor"),
];

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
  let place = Place {
    location: v.str("location").as_deref().and_then(coord),
    address: v.str("address"),
    district: v.str("district"),
    adcode: v.str("adcode"),
    ..Place::default()
  };
  let mut post = Post {
    id: id.clone().unwrap_or_else(|| name.clone()),
    kind: "tip".into(),
    title: Some(name),
    url: id.as_deref().map(place_url),
    place: Some(Box::new(place)),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(code) = v.str("typecode") {
    post.extra.insert("typecode".into(), code.into());
  }
  Some(post)
}

/// A place of reverse geocoding (`regeocode.pois`).
pub fn nearby_poi(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let place = Place {
    location: v.str("location").as_deref().and_then(coord),
    address: v.str("address"),
    category: v.str("type"),
    phone: v.str("tel"),
    distance_m: v.f64("distance").map(|d| d.round() as u64),
    ..Place::default()
  };
  Some(Post {
    url: Some(place_url(&id)),
    id,
    kind: "poi".into(),
    title: v.str("name"),
    place: Some(Box::new(place)),
    raw: Some(v.clone()),
    ..Post::default()
  })
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
  if let Some(center) = coord_at(v, "center") {
    c.extra.insert("center".into(), center);
  }
  for (key, value) in [
    ("citycode", v.str("citycode")),
    ("parent", parent.map(str::to_owned)),
  ] {
    if let Some(value) = value {
      c.extra.insert(key.into(), value.into());
    }
  }
  Some(c)
}
