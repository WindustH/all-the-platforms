//! Tencent answers → the core model, and route polylines decompressed.

use atp_core::geo::{Place, Polyline, System};
use atp_core::{Post, Value, ValueExt};

use crate::refs::{self, place_url};

/// A place of search, explore, details or reverse geocoding.
pub fn place(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let ad = v.at("ad_info");
  let place = Place {
    location: refs::located(v.at("location")),
    address: v.str("address"),
    category: v.str("category"),
    phone: v.str("tel"),
    distance_m: v.f64("_distance").map(|d| d.round() as u64),
    province: ad.str("province"),
    city: ad.str("city"),
    district: ad.str("district"),
    adcode: ad.str("adcode"),
    ..Place::default()
  };
  let mut post = Post {
    url: Some(place_url(&id)),
    id,
    kind: "poi".into(),
    title: v.str("title"),
    place: Some(Box::new(place)),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(direction) = v.str("_dir_desc") {
    post.extra.insert("direction".into(), direction.into());
  }
  Some(post)
}

/// A keyword suggestion: its region fields are flat.
pub fn suggestion(v: &Value) -> Option<Post> {
  let title = v.str("title")?;
  let id = v.str("id");
  let place = Place {
    location: refs::located(v.at("location")),
    address: v.str("address"),
    category: v.str("category"),
    distance_m: v.f64("_distance").map(|d| d.round() as u64),
    province: v.str("province"),
    city: v.str("city"),
    district: v.str("district"),
    adcode: v.str("adcode"),
    ..Place::default()
  };
  Some(Post {
    id: id.clone().unwrap_or_else(|| title.clone()),
    kind: "tip".into(),
    title: Some(title),
    url: id.as_deref().map(place_url),
    place: Some(Box::new(place)),
    raw: Some(v.clone()),
    ..Post::default()
  })
}

/// A route polyline (see [`decompress`](crate::polyline::decompress)).
pub fn polyline(v: &Value) -> Option<Polyline> {
  let numbers: Vec<f64> = v.as_array()?.iter().filter_map(Value::as_f64).collect();
  let points = crate::polyline::decompress(&numbers);
  (!points.is_empty()).then(|| Polyline::new(System::Gcj02, &points))
}
