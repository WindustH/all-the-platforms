//! Sights (景点) as posts: the city list, rankings, the sight page and its
//! detail modules; and the places of the search and suggest services.

use atp_core::text::html_to_text;
use atp_core::{Media, Post, Price, Value, ValueExt, json};

use super::{absolute, copy, ids, location, original, put, strings};
use crate::refs::{self, Sight};

/// A sight of a city's list (`18109/json/getAttractionList`, `card`).
pub fn sight_card(v: &Value) -> Option<Post> {
  let sight = Sight {
    sight: v.str("businessId"),
    district: v.str("districtId"),
    poi: v.str("poiId"),
  };
  let mut post = Post {
    id: sight.sight.clone().or_else(|| sight.poi.clone())?,
    kind: "sight".into(),
    title: v.str("poiName"),
    text: Some(card_text(v)).filter(|t| !t.is_empty()),
    url: Some(refs::sight_url(&sight)),
    price: card_price(v),
    media: v
      .str("coverImageUrl")
      .map(|u| Media::image(original(&u)))
      .into_iter()
      .collect(),
    tags: strings(v, "tagNameList", "name"),
    raw: Some(v.clone()),
    ..Post::default()
  };
  post.metrics.comments = v.count("commentCount");
  let extra = &mut post.extra;
  copy(extra, v, CARD_FIELDS);
  ids(
    extra,
    v,
    &[("poi_id", "poiId"), ("district_id", "districtId")],
  );
  coordinate(extra, v);
  Some(post)
}

/// Extra fields of a listed sight: (name in `extra`, path in the card).
const CARD_FIELDS: &[(&str, &str)] = &[
  ("score", "commentScore"),
  ("heat", "heatScore"),
  ("level", "sightLevelStr"),
  ("zone", "zoneName"),
  ("district", "districtName"),
];

/// A `coordinate` object (`latitude`, `longitude`, `coordinateType`).
fn coordinate(extra: &mut atp_core::Extra, v: &Value) {
  let system = v.str("coordinate.coordinateType");
  location(
    extra,
    v,
    "coordinate.latitude",
    "coordinate.longitude",
    system,
  );
}

/// Level, area, distance and features, a part each.
fn card_text(v: &Value) -> String {
  let mut parts: Vec<String> = ["sightLevelStr", "zoneName", "distanceStr"]
    .iter()
    .filter_map(|k| v.str(k))
    .collect();
  parts.extend(strings(v, "shortFeatures", "text"));
  parts.join(" · ")
}

/// The ticket price from (`门票起`), or free.
fn card_price(v: &Value) -> Option<Price> {
  let mut price = from_price(v, "isFree", "price")?;
  if price.amount > 0.0 {
    price.original = v.f64("marketPrice").filter(|m| *m > price.amount);
    let kind = v.str("priceTypeDesc").unwrap_or_default();
    price.label = Some(format!("{kind}起"));
  }
  Some(price)
}

/// An entry of a ranking page (`rankingItemInfo.itemList`).
pub fn ranking_item(v: &Value) -> Option<Post> {
  let base = v.at("basicInfo");
  let ext = v.at("extInfo");
  let kind = base.str("businessType").unwrap_or_else(|| "sight".into());
  let id = base.str("businessId")?;
  let url = base.str("jumpUrl.commonUrl").map(|u| absolute(&u));
  let mut post = Post {
    kind: kind.clone(),
    title: base.str("businessName"),
    text: base.str("recommendReason"),
    url: match kind.as_str() {
      "sight" => Some(refs::sight_url(&Sight {
        district: ext.str("districtInfo.id"),
        poi: Some(id.clone()),
        ..Sight::default()
      })),
      "hotel" => Some(refs::hotel_url(&id)),
      _ => url,
    },
    media: super::images(base, "imageUrlList", &["url"]),
    tags: [ext.str("sightLevel"), ext.str("location")]
      .into_iter()
      .flatten()
      .collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  post.metrics.comments = ext.count("commentInfo.commentCount");
  post.price = from_price(ext.at("priceInfo"), "isFree", "value");
  put(&mut post.extra, "rank", base.get("rank").cloned());
  if kind == "sight" {
    put(&mut post.extra, "poi_id", Some(post.id.clone().into()));
  }
  copy(&mut post.extra, ext, RANKED_FIELDS);
  Some(post)
}

/// Extra fields of a ranked sight: (name in `extra`, path in `extInfo`).
const RANKED_FIELDS: &[(&str, &str)] = &[
  ("score", "commentInfo.commentScore"),
  ("heat", "hotScore"),
  ("feature", "shortFeature"),
];

/// A price from (`起`), or free.
fn from_price(v: &Value, free: &str, amount: &str) -> Option<Price> {
  if v.bool(free) == Some(true) {
    return Some(Price {
      label: Some("免费".into()),
      ..Price::cny(0.0)
    });
  }
  let amount = v.f64(amount).filter(|p| *p > 0.0)?;
  Some(Price {
    label: Some("起".into()),
    ..Price::cny(amount)
  })
}

/// The sight page's state (`poiDetail`, `poiCommentInfo`).
pub fn sight_page(state: &Value) -> Option<Post> {
  let d = state.at("poiDetail");
  let sight = Sight {
    sight: d.str("businessId"),
    district: d.str("districtId"),
    poi: d.str("poiId"),
  };
  let info = state.at("poiCommentInfo");
  let scores = sub_scores(info);
  let mut post = Post {
    id: sight.sight.clone()?,
    kind: "sight".into(),
    title: d.str("poiName"),
    text: Some(page_text(d, &scores)),
    url: Some(refs::sight_url(&sight)),
    media: page_media(d),
    tags: strings(info, "subCommentHotTags", "tagContent"),
    raw: Some(state.clone()),
    ..Post::default()
  };
  post.metrics.comments = d.count("commentCount");
  let extra = &mut post.extra;
  put(extra, "scores", Some(scores.into()));
  put(
    extra,
    "level",
    d.u64("poiLevel").map(|l| format!("{l}A").into()),
  );
  copy(extra, d, PAGE_FIELDS);
  ids(
    extra,
    d,
    &[("poi_id", "poiId"), ("district_id", "districtId")],
  );
  Some(post)
}

/// Extra fields of the sight page: (name in `extra`, path in `poiDetail`).
const PAGE_FIELDS: &[(&str, &str)] = &[
  ("score", "commentScore"),
  ("heat", "heatScore"),
  ("english_name", "poiEName"),
  ("district", "districtName"),
  ("address", "address"),
  ("phones", "telephone"),
  ("hours", "openInfo.openTime"),
  ("play_time", "openInfo.playSpendTime"),
  ("ranking", "rankingDesc"),
  ("photos", "imageInfo.imageCount"),
];

/// `景色:4.8`-style sub-scores of the review summary.
fn sub_scores(info: &Value) -> Vec<Value> {
  info
    .list("scores")
    .iter()
    .filter_map(|s| Some(format!("{}:{}", s.str("name")?, s.str("score")?).into()))
    .collect()
}

fn page_text(d: &Value, scores: &[Value]) -> String {
  let scores: Vec<&str> = scores.iter().filter_map(Value::as_str).collect();
  let phones: Vec<String> = strings(d, "telephone", "");
  [
    d.str("address").map(|a| format!("地址 {a}")),
    d.str("openInfo.openTime").map(|h| format!("开放时间 {h}")),
    d.str("openInfo.playSpendTime")
      .map(|t| format!("建议游玩 {t}")),
    Some(phones.join(" "))
      .filter(|p| !p.is_empty())
      .map(|p| format!("电话 {p}")),
    d.str("rankingDesc"),
    Some(scores.join(" ")).filter(|s| !s.is_empty()),
    d.str("introduction")
      .map(|i| format!("\n{}", html_to_text(&i))),
  ]
  .into_iter()
  .flatten()
  .collect::<Vec<_>>()
  .join("\n")
}

fn page_media(d: &Value) -> Vec<Media> {
  let mut media = super::images(d, "imageInfo.poiPhotoImageList", &["imageUrl", "url"]);
  if let Some(video) = d.str("imageInfo.liveTelecastUrl") {
    media.push(Media::video(absolute(&video)));
  }
  media
}

/// Add the detail modules of `18254/json/getPoiMoreDetail`: coordinates,
/// opening status, tickets and facilities.
pub fn sight_more(post: &mut Post, v: &Value) {
  let modules: Vec<&Value> = v
    .list("templateList")
    .iter()
    .flat_map(|t| t.list("moduleList"))
    .collect();
  let find = |key: &str| modules.iter().map(|m| m.at(key)).find(|x| x.is_object());
  let extra = &mut post.extra;
  if let Some(basic) = find("poiBasicModule") {
    coordinate(extra, basic);
  }
  if let Some(open) = find("poiOpenModule") {
    copy(extra, open, &[("open_status", "openStatus")]);
  }
  if let Some(ticket) = find("ticketAndAppointmentModule") {
    copy(extra, ticket, &[("tickets", "ticketDesc")]);
  }
  if let Some(facility) = find("serviceFacilityModule") {
    let names = strings(facility, "serviceFacilityModule", "facilityName");
    put(extra, "facilities", Some(json!(names)));
  }
}

/// A place of the global search (`26872/search`).
pub fn search_item(v: &Value) -> Option<Post> {
  let mut kind = v.str("type")?;
  let id = v.str("id").filter(|id| id != "0")?;
  if kind == "sightplay" {
    kind = "sight".into();
  }
  let url = match kind.as_str() {
    "sight" => Some(refs::sight_url(&Sight {
      sight: Some(id.clone()),
      district: v.str("districtId"),
      poi: v.str("poiId"),
    })),
    "hotel" => Some(refs::hotel_url(&id)),
    _ => v.str("url").map(|u| absolute(&u)),
  };
  let text = [
    v.str("zoneName"),
    v.str("locationName"),
    v.str("starRating"),
  ]
  .into_iter()
  .flatten()
  .collect::<Vec<_>>()
  .join(" · ");
  let mut post = Post {
    kind,
    title: v.str("word"),
    text: Some(text).filter(|t| !t.is_empty()),
    url,
    price: v.f64("price").filter(|p| *p > 0.0).map(|p| Price {
      label: Some("起".into()),
      ..Price::cny(p)
    }),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  place_extra(&mut post, v);
  Some(post)
}

fn place_extra(post: &mut Post, v: &Value) {
  let extra = &mut post.extra;
  copy(extra, v, &[("city", "cityName"), ("category", "childType")]);
  let fields = [
    ("city_id", "cityId"),
    ("district_id", "districtId"),
    ("poi_id", "poiId"),
  ];
  ids(extra, v, &fields);
  location(extra, v, "lat", "lon", None);
}

/// A destination, station or hotel suggested for a keyword
/// (`21881/json/gaHotelSearchEngine`).
pub fn suggestion(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let kind = v.str("type").unwrap_or_default().to_ascii_lowercase();
  let url = (kind == "hotel").then(|| refs::hotel_url(&id));
  let mut post = Post {
    kind,
    title: v.str("word"),
    text: v.first_str(&["displayName", "displayType"]),
    url,
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  let extra = &mut post.extra;
  copy(extra, v, SUGGESTION_FIELDS);
  ids(extra, v, &[("city_id", "cityId")]);
  location(extra, v, "gdLat", "gdLon", Some("GCJ02".into()));
  Some(post)
}

/// Extra fields of a suggestion: (name in `extra`, path in the entry).
const SUGGESTION_FIELDS: &[(&str, &str)] = &[
  ("type", "displayType"),
  ("city", "cityName"),
  ("province", "provinceName"),
  ("country", "countryName"),
  ("english_name", "eName"),
];
