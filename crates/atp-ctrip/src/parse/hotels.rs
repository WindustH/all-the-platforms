//! Hotels as posts: the city list page and the hotel page (both read from
//! their RSC data), and the rooms of a logged-in room search.

use atp_core::{Media, Post, Price, Value, ValueExt, json};

use super::{absolute, copy, location, original, put, strings};
use crate::refs;

/// A hotel of the list page (`initListData.hotelList`). Visitors get no
/// prices here.
pub fn hotel_item(v: &Value) -> Option<Post> {
  let h = v.at("hotelInfo");
  let id = h.first_str(&["summary.masterHotelId", "summary.hotelId"])?;
  let c = h.at("commentInfo");
  let position = [
    h.str("positionInfo.positionDesc"),
    h.str("positionInfo.address"),
  ];
  let mut post = Post {
    kind: "hotel".into(),
    title: h.str("nameInfo.name"),
    text: position
      .into_iter()
      .flatten()
      .reduce(|a, b| format!("{a} · {b}")),
    url: Some(refs::hotel_url(&id)),
    price: list_price(v),
    media: super::images(h, "hotelImages.multiImgs", &["url"]),
    tags: strings(c, "oneSentenceComment", "tagTitle")
      .into_iter()
      .map(|t| t.trim_matches('"').to_owned())
      .collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  post.metrics.comments = c.str("commenterNumber").and_then(|n| digits(&n));
  let extra = &mut post.extra;
  copy(extra, h, ITEM_FIELDS);
  put(
    extra,
    "scores",
    Some(scores(c.list("subScore"), "content", "number")),
  );
  put(
    extra,
    "room",
    Some(v.at("roomInfo.0.summary.saleRoomName").clone()),
  );
  let coord = h.at("positionInfo.mapCoordinate.0");
  location(
    extra,
    coord,
    "latitude",
    "longitude",
    coord.str("gcoordType"),
  );
  Some(post)
}

/// Extra fields of a listed hotel: (name in `extra`, path in `hotelInfo`).
const ITEM_FIELDS: &[(&str, &str)] = &[
  ("score", "commentInfo.commentScore"),
  ("rating", "commentInfo.commentDescription"),
  ("stars", "hotelStar.star"),
  ("category", "hotelCategory.categoryName"),
  ("zones", "positionInfo.zoneNames"),
];

/// `卫生:4.8`-style sub-scores.
fn scores(list: &[Value], name: &str, score: &str) -> Value {
  let list: Vec<Value> = list
    .iter()
    .filter_map(|s| Some(format!("{}:{}", s.str(name)?, s.str(score)?).into()))
    .collect();
  list.into()
}

/// The number in `4,060条点评`.
fn digits(text: &str) -> Option<u64> {
  let digits: String = text.chars().filter(char::is_ascii_digit).collect();
  digits.parse().ok()
}

/// The lowest price the list shows, when it shows one (logged in).
fn list_price(v: &Value) -> Option<Price> {
  let p = v.at("roomInfo.0.priceInfo");
  let amount = p
    .f64("displayPriceDetail.price")
    .or_else(|| p.f64("price"))
    .filter(|a| *a > 0.0)?;
  Some(Price {
    label: Some("起".into()),
    ..Price::cny(amount)
  })
}

/// The hotel page (`hotelDetailResponse`).
pub fn hotel(d: &Value, id: &str) -> Option<Post> {
  let base = d.at("hotelBaseInfo");
  let name = base.str("nameInfo.name")?;
  let comment = d.at("hotelComment.comment");
  let scores = scores(comment.list("scoreDetail"), "showName", "showScore");
  let mut post = Post {
    id: base.str("masterHotelId").unwrap_or_else(|| id.to_owned()),
    kind: "hotel".into(),
    title: Some(name),
    text: Some(hotel_text(d, &scores)),
    url: Some(refs::hotel_url(id)),
    media: hotel_media(d),
    tags: strings(comment, "quality", ""),
    raw: Some(d.clone()),
    ..Post::default()
  };
  post.metrics.comments = comment.count("totalComment");
  let extra = &mut post.extra;
  copy(extra, d, HOTEL_FIELDS);
  put(extra, "scores", Some(scores));
  let facilities = strings(d, "hotelFacilityBelt.facilityList", "facilityDesc");
  put(extra, "facilities", Some(json!(facilities)));
  let phones = strings(d, "hotelDescriptionInfo.tels", "show");
  put(extra, "phones", Some(json!(phones)));
  put(
    extra,
    "check_in_out",
    Some(json!(policy(d, "checkInAndOut"))),
  );
  let position = d.at("hotelPositionInfo");
  let system = position.str("mapType").map(|t| match t.as_str() {
    "bd" => "BD09".to_owned(),
    "gd" => "GCJ02".to_owned(),
    _ => t.to_ascii_uppercase(),
  });
  location(extra, position, "lat", "lng", system);
  Some(post)
}

/// Extra fields of the hotel page: (name in `extra`, path in the page data).
const HOTEL_FIELDS: &[(&str, &str)] = &[
  ("score", "hotelComment.comment.score"),
  ("rating", "hotelComment.comment.scoreDescription"),
  ("stars", "hotelBaseInfo.starInfo.level"),
  ("city", "hotelBaseInfo.cityName"),
  ("city_id", "hotelBaseInfo.cityId"),
  ("address", "hotelPositionInfo.address"),
  ("facts", "hotelDescriptionInfo.lables"),
];

/// Lines of one entry of the hotel's policies.
fn policy(d: &Value, key: &str) -> Vec<String> {
  strings(d.at("hotelPolicyInfo").at(key), "content", "description")
}

fn hotel_text(d: &Value, scores: &Value) -> String {
  let scores: Vec<&str> = scores.list("").iter().filter_map(Value::as_str).collect();
  let description: Vec<String> = strings(d, "hotelDescriptionInfo.sectionList", "desc");
  [
    d.str("hotelPositionInfo.address")
      .map(|a| format!("地址 {a}")),
    Some(policy(d, "checkInAndOut").join(" ")).filter(|p| !p.is_empty()),
    Some(scores.join(" ")).filter(|s| !s.is_empty()),
    Some(strings(d, "hotelDescriptionInfo.lables", "").join(" · ")).filter(|l| !l.is_empty()),
    Some(description.join("\n"))
      .filter(|t| !t.is_empty())
      .map(|t| format!("\n{t}")),
  ]
  .into_iter()
  .flatten()
  .collect::<Vec<_>>()
  .join("\n")
}

fn hotel_media(d: &Value) -> Vec<Media> {
  let top = d.list("hotelTopImage.imgUrlList");
  let mut media: Vec<Media> = top
    .iter()
    .filter_map(|i| i.str("imgUrl"))
    .map(|u| Media::image(original(&u)))
    .collect();
  media.extend(
    top
      .iter()
      .filter_map(|i| i.str("videoUrl"))
      .map(|u| Media::video(absolute(&u))),
  );
  media
}

/// A room plan of a room search (`saleRoomMap` entry), with its room
/// (`physicRoomMap` entry).
pub fn room(sale: &Value, physical: &Value, hotel: &str) -> Option<Post> {
  let id = sale.first_str(&["id", "saleRoomId", "roomId"])?;
  let name = physical.str("name").or_else(|| sale.str("name"))?;
  let plan = sale.str("name").filter(|n| *n != name);
  let room = ["bedInfo", "areaInfo", "floorInfo", "windowInfo"].map(|k| (physical, k));
  let terms = ["mealInfo", "cancelInfo"].map(|k| (sale, k));
  let details: Vec<String> = room
    .iter()
    .chain(&terms)
    .filter_map(|(v, k)| v.str(&format!("{k}.title")))
    .collect();
  let mut post = Post {
    kind: "room".into(),
    title: Some(match plan {
      Some(plan) => format!("{name} · {plan}"),
      None => name,
    }),
    text: Some(details.join(" · ")).filter(|t| !t.is_empty()),
    url: Some(refs::hotel_url(hotel)),
    price: room_price(sale),
    raw: Some(json!({ "sale": sale, "room": physical })),
    id,
    ..Post::default()
  };
  copy(&mut post.extra, sale, ROOM_FIELDS);
  Some(post)
}

/// Extra fields of a room plan: (name in `extra`, path in the plan).
const ROOM_FIELDS: &[(&str, &str)] = &[
  ("remaining", "bookingStatusInfo.remainRoomQuantity"),
  ("bookable", "bookingStatusInfo.isBooking"),
  ("full", "bookingStatusInfo.isFullRoom"),
  ("meal", "mealInfo.title"),
  ("cancel", "cancelInfo.title"),
];

/// The price of a night, the crossed-out one and the stay's total.
fn room_price(sale: &Value) -> Option<Price> {
  let amount = sale.f64("priceInfo.price").filter(|p| *p > 0.0)?;
  Some(Price {
    original: sale
      .f64("priceInfo.deletePricewithOutCurrency")
      .filter(|o| *o > amount),
    label: sale.str("totalPriceInfo.total.content"),
    ..Price::cny(amount)
  })
}
