//! 美团联盟 deals (`query_coupon` `data[]`) → posts. A deal's id is its
//! `productViewSign` (older answers: `couponPackDetail.skuViewId`); its
//! price is `sellPrice`, the crossed-out one `originalPrice`. The shop is
//! the nearest delivering one (到家, with a location) or else the brand.

use atp_core::text::{from_secs, parse_count};
use atp_core::{Media, Post, Price, User, Value, ValueExt, json};

/// Labels Meituan puts on a deal (`点评高分4.6`, rankings, price power).
const LABELS: [&str; 3] = ["dianPingRankLabel", "productRankLabel", "pricePowerLabel"];

pub fn deal(v: &Value) -> Option<Post> {
  let detail = v.at("couponPackDetail");
  let id = v.first_str(&[
    "productViewSign",
    "couponPackDetail.productViewSign",
    "couponPackDetail.skuViewId",
    "skuViewId",
  ])?;
  let tags: Vec<String> = LABELS.iter().filter_map(|k| v.str(k)).collect();
  let mut post = Post {
    id,
    kind: "deal".into(),
    title: detail.str("name"),
    text: detail.str("specification"),
    author: shop(v),
    price: price(v),
    media: detail
      .str("headUrl")
      .map(Media::image)
      .into_iter()
      .collect(),
    tags,
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(n) = detail.str("saleVolume").and_then(|s| parse_count(&s)) {
    post.metrics.other.insert("sold".into(), n);
  }
  extra(&mut post, v);
  Some(post)
}

/// A number at the first of `paths` that holds one.
pub fn number(v: &Value, paths: &[&str]) -> Option<f64> {
  paths.iter().find_map(|p| v.f64(p))
}

fn price(v: &Value) -> Option<Price> {
  let amount = number(v, &["couponPackDetail.sellPrice", "sellPrice"])?;
  let original = number(
    v,
    &[
      "couponPackDetail.originalPrice",
      "couponPackDetail.originalPrlice",
      "originalPrice",
    ],
  );
  Some(Price {
    original: original.filter(|o| *o > amount),
    ..Price::cny(amount)
  })
}

fn shop(v: &Value) -> Option<User> {
  let delivering = ["deliverablePoiInfo", "couponPackDetail.deliverablePoiInfo"]
    .iter()
    .map(|k| v.at(k))
    .find(|p| p.str("poiName").is_some());
  let (name, avatar, kind) = match delivering {
    Some(p) => (
      p.str("poiName")?,
      p.first_str(&["poiLogoUrl", "poiLogoURL"]),
      "shop",
    ),
    None => (
      v.str("brandInfo.brandName")?,
      v.str("brandInfo.brandLogoUrl"),
      "brand",
    ),
  };
  let mut user = User {
    id: name.clone(),
    name,
    avatar,
    ..User::default()
  };
  user.extra.insert("kind".into(), kind.into());
  Some(user)
}

fn extra(post: &mut Post, v: &Value) {
  let detail = v.at("couponPackDetail");
  let platform = match detail.i64("platform") {
    Some(2) => Some("store"),
    Some(1) => Some("home"),
    _ => None,
  };
  let delivery = ["deliverablePoiInfo", "couponPackDetail.deliverablePoiInfo"]
    .iter()
    .map(|k| v.at(k))
    .find(|p| p.is_object())
    .map(|p| {
      json!({
        "distance": p.f64("deliveryDistance"),
        "fee": p.f64("distributionCost"),
        "minutes": p.i64("deliveryDuration"),
        "minimum_order": p.f64("lastDeliveryFee"),
      })
    });
  let fields = [
    ("platform", platform.map(Value::from)),
    ("brand", v.str("brandInfo.brandName").map(Value::from)),
    (
      "commission_percent",
      v.f64("commissionInfo.commissionPercent")
        .map(|p| (p / 100.0).into()),
    ),
    (
      "commission",
      v.f64("commissionInfo.commission").map(Value::from),
    ),
    (
      "shops_available",
      v.i64("availablePoiInfo.availablePoiNum").map(Value::from),
    ),
    ("coupons", detail.i64("couponNum").map(Value::from)),
    ("on_sale", detail.bool("saleStatus").map(Value::from)),
    ("valid_from", time(detail, "startTime")),
    ("valid_until", time(detail, "endTime")),
    (
      "daily_limit",
      v.i64("purchaseLimitInfo.singleDayPurchaseLimit")
        .map(Value::from),
    ),
    ("delivery", delivery),
  ];
  for (key, value) in fields {
    if let Some(value) = value.filter(|x| !x.is_null()) {
      post.extra.insert(key.into(), value);
    }
  }
}

fn time(v: &Value, key: &str) -> Option<Value> {
  from_secs(v.i64(key)?).map(|t| t.to_string().into())
}
