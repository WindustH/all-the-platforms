//! Google answers → the core model: places of Places (New), suggestions,
//! geocoding results, durations and money.

use atp_core::geo::Place;
use atp_core::{Media, Post, Price, Value, ValueExt};

use crate::api::PLACES;
use crate::refs::{self, place_url};

/// A place of Places (New): search results and details.
pub fn place(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let mut place = Place {
    location: refs::point(v.at("location")),
    address: v.str("formattedAddress"),
    category: v.str("primaryTypeDisplayName.text"),
    phone: v.first_str(&["internationalPhoneNumber", "nationalPhoneNumber"]),
    rating: v.f64("rating"),
    hours: hours(v),
    website: v.str("websiteUri"),
    ..Place::default()
  };
  regions(&mut place, v.list("addressComponents"), "longText");
  let mut post = Post {
    kind: "poi".into(),
    title: v.str("displayName.text"),
    url: Some(place_url(&id)),
    place: Some(Box::new(place)),
    media: v.list("photos").iter().filter_map(photo).collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  post.id = id;
  for (key, path) in [
    ("rating_count", "userRatingCount"),
    ("status", "businessStatus"),
    ("price_level", "priceLevel"),
    ("maps_url", "googleMapsUri"),
  ] {
    if let Some(value) = v.str(path) {
      post.extra.insert(key.into(), value.into());
    }
  }
  Some(post)
}

fn hours(v: &Value) -> Option<String> {
  let days: Vec<String> = v
    .list("regularOpeningHours.weekdayDescriptions")
    .iter()
    .filter_map(|d| d.as_str().map(str::to_owned))
    .collect();
  (!days.is_empty()).then(|| days.join("; "))
}

/// Country, province, city and district from address components; `text`
/// names the field holding the name (`longText` or `long_name`).
pub fn regions(place: &mut Place, components: &[Value], text: &str) {
  for c in components {
    let Some(name) = c.str(text) else {
      continue;
    };
    let types: Vec<&str> = c.list("types").iter().filter_map(Value::as_str).collect();
    let slot = if types.contains(&"country") {
      &mut place.country
    } else if types.contains(&"administrative_area_level_1") {
      &mut place.province
    } else if types.contains(&"locality") {
      &mut place.city
    } else if types.contains(&"sublocality_level_1")
      || types.contains(&"administrative_area_level_2")
    {
      &mut place.district
    } else {
      continue;
    };
    if slot.is_none() {
      *slot = Some(name);
    }
  }
}

/// A photo as its resource; `download` turns it into a short-lived link.
fn photo(p: &Value) -> Option<Media> {
  let name = p.str("name")?;
  let mut m = Media::image(format!("{PLACES}/{name}/media"));
  m.width = p.u64("widthPx").map(|w| w as u32);
  m.height = p.u64("heightPx").map(|h| h as u32);
  m.alt = p
    .str("authorAttributions.0.displayName")
    .map(|a| format!("photo by {a}"));
  Some(m)
}

/// An autocomplete suggestion: a place, or a query to search for.
pub fn suggestion(s: &Value) -> Option<Post> {
  if let Some(text) = s.str("queryPrediction.text.text") {
    return Some(Post {
      id: text.clone(),
      kind: "query".into(),
      title: Some(text),
      // An empty place keeps suggestions in one table.
      place: Some(Box::default()),
      ..Post::default()
    });
  }
  let p = s.at("placePrediction");
  let id = p.str("placeId")?;
  let place = Place {
    address: p.str("structuredFormat.secondaryText.text"),
    distance_m: p.u64("distanceMeters"),
    category: p
      .list("types")
      .first()
      .and_then(|t| t.as_str())
      .map(str::to_owned),
    ..Place::default()
  };
  Some(Post {
    url: Some(place_url(&id)),
    id,
    kind: "tip".into(),
    title: p.first_str(&["structuredFormat.mainText.text", "text.text"]),
    place: Some(Box::new(place)),
    raw: Some(s.clone()),
    ..Post::default()
  })
}

/// A Geocoding API result as an address.
pub fn geocoded(r: &Value) -> Option<Post> {
  let location = refs::point(r.at("geometry.location"))?;
  let id = r.str("place_id").unwrap_or_else(|| location.lng_lat());
  let mut place = Place {
    location: Some(location),
    address: r.str("formatted_address"),
    category: r
      .list("types")
      .first()
      .and_then(|t| t.as_str())
      .map(str::to_owned),
    ..Place::default()
  };
  regions(&mut place, r.list("address_components"), "long_name");
  let mut post = Post {
    url: r.str("place_id").as_deref().map(place_url),
    id,
    kind: "address".into(),
    title: place.address.clone(),
    place: Some(Box::new(place)),
    raw: Some(r.clone()),
    ..Post::default()
  };
  if let Some(precision) = r.str("geometry.location_type") {
    post.extra.insert("precision".into(), precision.into());
  }
  if r.bool("partial_match") == Some(true) {
    post.extra.insert("partial_match".into(), true.into());
  }
  Some(post)
}

/// `"165s"` → 165.
pub fn seconds(v: &Value, path: &str) -> Option<u64> {
  let text = v.str(path)?;
  let secs: f64 = text.trim_end_matches('s').parse().ok()?;
  Some(secs.round() as u64)
}

/// Google's Money `{currencyCode, units, nanos}`.
pub fn money(v: &Value) -> Option<Price> {
  let currency = v.str("currencyCode")?;
  let units = v.f64("units").unwrap_or(0.0);
  let nanos = v.f64("nanos").unwrap_or(0.0);
  Some(Price {
    amount: units + nanos / 1e9,
    currency,
    ..Price::default()
  })
}
