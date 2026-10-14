//! Baidu answers → the core model. `system` is the system Baidu was asked
//! to answer in (`ret_coordtype`).

use atp_core::geo::{Place, System};
use atp_core::{Post, Value, ValueExt, json};

use crate::refs::{self, place_url};

/// A place of place search or details; the `detail_info` fields come with
/// `scope=2`.
pub fn place(v: &Value, system: System) -> Option<Post> {
  let uid = v.str("uid")?;
  let d = v.at("detail_info");
  let place = Place {
    location: refs::located(v.at("location"), system),
    address: v.str("address"),
    category: d.first_str(&["tag", "classified_poi_tag"]),
    phone: v.str("telephone"),
    rating: d.f64("overall_rating"),
    distance_m: d.f64("distance").map(|m| m.round() as u64),
    province: v.str("province"),
    city: v.str("city"),
    district: v.str("area"),
    adcode: v.str("adcode"),
    hours: d.str("shop_hours"),
    ..Place::default()
  };
  let mut post = Post {
    url: Some(place_url(&uid)),
    id: uid,
    kind: "poi".into(),
    title: v.str("name"),
    place: Some(Box::new(place)),
    raw: Some(v.clone()),
    ..Post::default()
  };
  for (key, value) in [
    ("type", d.str("type")),
    ("price", d.str("price")),
    ("comments", d.str("comment_num")),
    ("street_id", v.str("street_id")),
  ] {
    if let Some(value) = value {
      post.extra.insert(key.into(), value.into());
    }
  }
  if let Some(entrance) = refs::located(d.at("navi_location"), system) {
    post.extra.insert("entrance".into(), json!(entrance));
  }
  Some(post)
}

/// A suggestion of place/v2/suggestion.
pub fn suggestion(v: &Value, system: System) -> Option<Post> {
  let name = v.str("name")?;
  let uid = v.str("uid");
  let place = Place {
    location: refs::located(v.at("location"), system),
    address: v.str("address"),
    category: v.str("tag"),
    province: v.str("province"),
    city: v.str("city"),
    district: v.str("district"),
    adcode: v.str("adcode"),
    ..Place::default()
  };
  Some(Post {
    id: uid.clone().unwrap_or_else(|| name.clone()),
    kind: "tip".into(),
    title: Some(name),
    url: uid.as_deref().map(place_url),
    place: Some(Box::new(place)),
    raw: Some(v.clone()),
    ..Post::default()
  })
}

/// A place listed by reverse geocoding (`pois`).
pub fn nearby(v: &Value, system: System) -> Option<Post> {
  let uid = v.str("uid")?;
  let place = Place {
    location: refs::located(v.at("point"), system),
    address: v.str("addr"),
    category: v.first_str(&["tag", "poiType"]),
    phone: v.str("tel"),
    distance_m: v.f64("distance").map(|m| m.round() as u64),
    ..Place::default()
  };
  Some(Post {
    url: Some(place_url(&uid)),
    id: uid,
    kind: "poi".into(),
    title: v.str("name"),
    place: Some(Box::new(place)),
    raw: Some(v.clone()),
    ..Post::default()
  })
}
