//! Places API (New): text search, nearby search, details, autocomplete and
//! photos. Lists ask only for Pro-tier fields; details add the Enterprise
//! ones (phone, rating, hours, website).

use atp_core::geo::Coord;
use atp_core::{Error, Media, Page, PageReq, Post, Query, Result, Value, ValueExt, json};

use crate::api::{Api, PLACES};
use crate::{parse, refs};

/// Fields of list items (Pro tier).
const LIST_FIELDS: &str = "places.id,places.displayName,places.formattedAddress,places.location,\
   places.primaryTypeDisplayName,places.googleMapsUri";
/// Fields of `read` (Enterprise tier: phone, rating, hours, website).
const DETAIL_FIELDS: &str = "id,displayName,formattedAddress,location,primaryTypeDisplayName,\
   googleMapsUri,addressComponents,businessStatus,internationalPhoneNumber,rating,\
   userRatingCount,priceLevel,regularOpeningHours.weekdayDescriptions,websiteUri,photos";
const SUGGESTION_FIELDS: &str = "suggestions.placePrediction.placeId,\
   suggestions.placePrediction.text,suggestions.placePrediction.structuredFormat,\
   suggestions.placePrediction.types,suggestions.placePrediction.distanceMeters,\
   suggestions.queryPrediction.text";

/// `--filter` names and the place types (Table A) they stand for.
pub const CATEGORIES: &[(&str, &str)] = &[
  ("food", "restaurant"),
  ("cafe", "cafe"),
  ("hotel", "lodging"),
  ("shopping", "shopping_mall"),
  ("supermarket", "supermarket"),
  ("medical", "hospital"),
  ("pharmacy", "pharmacy"),
  ("scenic", "tourist_attraction"),
  ("museum", "museum"),
  ("park", "park"),
  ("education", "school"),
  ("transport", "transit_station"),
  ("gas", "gas_station"),
  ("charging", "electric_vehicle_charging_station"),
  ("parking", "parking"),
  ("subway", "subway_station"),
  ("bus", "bus_station"),
  ("airport", "airport"),
  ("train", "train_station"),
  ("bank", "bank"),
  ("atm", "atm"),
];

pub const FILTERS: &[&str] = &[
  "food",
  "cafe",
  "hotel",
  "shopping",
  "supermarket",
  "medical",
  "pharmacy",
  "scenic",
  "museum",
  "park",
  "education",
  "transport",
  "gas",
  "charging",
  "parking",
  "subway",
  "bus",
  "airport",
  "train",
  "bank",
  "atm",
];

/// The place type of a `--filter` name, or the value itself (a raw type).
pub fn place_type(filter: &str) -> String {
  CATEGORIES
    .iter()
    .find(|(name, _)| *name == filter)
    .map_or_else(|| filter.to_owned(), |(_, t)| (*t).to_owned())
}

/// Text Search: the keyword with `city:NAME` appended to the text and
/// `type:TYPE` taken out as the place type.
pub async fn search(api: &Api, query: &Query, req: &PageReq) -> Result<Page<Post>> {
  let mut words = Vec::new();
  let mut kind = query.filter.as_deref().map(place_type);
  for word in query.keyword.split_whitespace() {
    if let Some(city) = word.strip_prefix("city:") {
      words.push(city.to_owned());
    } else if let Some(t) = word.strip_prefix("type:") {
      kind = Some(t.to_owned());
    } else {
      words.push(word.to_owned());
    }
  }
  if words.is_empty() {
    return Err(Error::input("give a keyword"));
  }
  let mut body = json!({ "textQuery": words.join(" ") });
  if let Some(kind) = kind {
    body["includedType"] = kind.into();
  }
  text_search(api, body, None, req).await
}

/// Text Search, one page; `center` measures each place's distance.
async fn text_search(
  api: &Api,
  mut body: Value,
  center: Option<Coord>,
  req: &PageReq,
) -> Result<Page<Post>> {
  body["pageSize"] = req.size_within(20).into();
  if let Some(token) = &req.cursor {
    body["pageToken"] = token.clone().into();
  }
  if let Some(language) = &api.language {
    body["languageCode"] = language.clone().into();
  }
  let fields = format!("{LIST_FIELDS},nextPageToken");
  let v = api
    .post(&format!("{PLACES}/places:searchText"), &fields, &body)
    .await?;
  let items = places(&v, center);
  Ok(Page::new(items, v.str("nextPageToken")))
}

fn places(v: &Value, center: Option<Coord>) -> Vec<Post> {
  let mut items: Vec<Post> = v.list("places").iter().filter_map(parse::place).collect();
  if let Some(center) = center {
    for post in &mut items {
      if let Some(place) = post.place.as_mut()
        && let Some(at) = place.location
      {
        place.distance_m = Some(center.distance_m(&at).round() as u64);
      }
    }
  }
  items
}

/// Places around a point: Nearby Search (nearest first) without a keyword,
/// Text Search biased to the circle with one.
pub async fn nearby(
  api: &Api,
  location: &str,
  keyword: Option<&str>,
  filter: Option<&str>,
  radius: u32,
  req: &PageReq,
) -> Result<Page<Post>> {
  let center = refs::expect_point(location, api.system())?;
  let circle =
    json!({ "circle": { "center": refs::lat_lng(center), "radius": radius.clamp(1, 50_000) } });
  if let Some(keyword) = keyword {
    let mut body = json!({ "textQuery": keyword, "locationBias": circle });
    if let Some(filter) = filter {
      body["includedType"] = place_type(filter).into();
    }
    return text_search(api, body, Some(center), req).await;
  }
  if req.cursor.is_some() {
    return Ok(Page::last(Vec::new()));
  }
  let mut body = json!({
    "locationRestriction": circle,
    "maxResultCount": req.size_within(20),
    "rankPreference": "DISTANCE",
  });
  if let Some(filter) = filter {
    body["includedTypes"] = json!([place_type(filter)]);
  }
  if let Some(language) = &api.language {
    body["languageCode"] = language.clone().into();
  }
  let v = api
    .post(&format!("{PLACES}/places:searchNearby"), LIST_FIELDS, &body)
    .await?;
  Ok(Page::last(places(&v, Some(center))))
}

/// Place Details.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = refs::place_id(arg)?;
  let v = details(api, &id, DETAIL_FIELDS).await?;
  parse::place(&v).ok_or_else(|| Error::not_found(format!("no Google place {id}")))
}

async fn details(api: &Api, id: &str, fields: &str) -> Result<Value> {
  let query: Vec<(&str, String)> = api
    .language
    .iter()
    .map(|l| ("languageCode", l.clone()))
    .collect();
  api
    .fetch(&format!("{PLACES}/places/{id}"), fields, &query)
    .await
}

/// A place and its photos as short-lived links (Place Details IDs Only,
/// then one Place Photos request per photo).
pub async fn photos(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let id = refs::place_id(arg)?;
  let v = details(api, &id, "id,displayName,photos").await?;
  let post = parse::place(&v).ok_or_else(|| Error::not_found(format!("no Google place {id}")))?;
  let mut media = Vec::new();
  for photo in &post.media {
    let query = [
      ("maxWidthPx", "4800".to_owned()),
      ("skipHttpRedirect", "true".to_owned()),
    ];
    let v = api.fetch(&photo.url, "", &query).await?;
    if let Some(uri) = v.str("photoUri") {
      let mut m = Media::image(uri);
      m.alt = photo.alt.clone();
      media.push(m);
    }
  }
  Ok((post, media))
}

/// Autocomplete: places and queries for what has been typed so far.
pub async fn tips(
  api: &Api,
  keyword: &str,
  city: Option<&str>,
  location: Option<&str>,
) -> Result<Page<Post>> {
  let input = match city {
    Some(city) => format!("{keyword} {city}"),
    None => keyword.to_owned(),
  };
  let mut body = json!({ "input": input, "includeQueryPredictions": true });
  if let Some(location) = location {
    let at = refs::expect_point(location, api.system())?;
    body["origin"] = refs::lat_lng(at);
    body["locationBias"] = json!({ "circle": { "center": refs::lat_lng(at), "radius": 50_000.0 } });
  }
  if let Some(language) = &api.language {
    body["languageCode"] = language.clone().into();
  }
  let v = api
    .post(
      &format!("{PLACES}/places:autocomplete"),
      SUGGESTION_FIELDS,
      &body,
    )
    .await?;
  let items = v
    .list("suggestions")
    .iter()
    .filter_map(parse::suggestion)
    .collect();
  Ok(Page::last(items))
}

/// The cheapest check of a key: an IDs-only Text Search, which Google does
/// not bill.
pub async fn check(api: &Api) -> Result<()> {
  let body = json!({ "textQuery": "coffee", "pageSize": 1 });
  api
    .post(&format!("{PLACES}/places:searchText"), "places.id", &body)
    .await
    .map(|_| ())
}
