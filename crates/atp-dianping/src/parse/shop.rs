//! A shop from the mobile shop page's cached calls: `unify/shop.bin` (name,
//! scores, spend, hours, photos; phone and address come masked to
//! visitors), `rankinfo` (the ranking it is on), `shopinfo` (recommended
//! dishes) and `dealfilterlist` (how many deals it sells).

use atp_core::text::parse_count;
use atp_core::{Media, Post, Price, Value, ValueExt};
use atp_mtsec::shops;

use super::cached;

pub fn shop(cache: &Value) -> Option<Post> {
  let v = cached(cache, "unify/shop.bin")?;
  let id = v.str("id")?;
  let name = v.str("name")?;
  let title = match v.str("branchName") {
    Some(branch) => format!("{name}({branch})"),
    None => name,
  };
  let mut post = Post {
    kind: "shop".into(),
    title: Some(title),
    text: Some(summary(v)).filter(|t| !t.is_empty()),
    url: Some(shops::mobile_url(&id)),
    price: price(v),
    media: media(v),
    tags: tags(v, cache),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  if let Some(n) = v.count("picCount") {
    post.metrics.other.insert("photos".into(), n);
  }
  if let Some(n) = v.str("videoCountStr").and_then(|s| parse_count(&s)) {
    post.metrics.other.insert("videos".into(), n);
  }
  extra(&mut post, v, cache);
  Some(post)
}

/// Category and region, spend, hours, status and sub-scores, a line each.
fn summary(v: &Value) -> String {
  let place: Vec<String> = ["categoryName", "regionName"]
    .iter()
    .filter_map(|k| v.str(k))
    .collect();
  let scores: Vec<String> = v
    .list("scoreTextList")
    .iter()
    .filter_map(|s| s.as_str().map(str::to_owned))
    .collect();
  [
    Some(place.join(" · ")),
    v.str("priceText"),
    hours(v).map(|h| format!("营业时间 {h}")),
    v.str("bizStatusTime.bizStatus"),
    Some(scores.join(" ")),
    v.str("address").map(|a| format!("地址 {a}")),
  ]
  .into_iter()
  .flatten()
  .filter(|line| !line.is_empty())
  .collect::<Vec<_>>()
  .join("\n")
}

fn hours(v: &Value) -> Option<String> {
  v.list("shopExtraInfo.shopExtraInfos")
    .iter()
    .find(|i| i.str("name").as_deref() == Some("营业时间"))
    .and_then(|i| i.str("value"))
    .map(|h| h.replace('\n', " "))
}

/// Spend per person (人均).
fn price(v: &Value) -> Option<Price> {
  let amount = v.f64("avgPrice").filter(|p| *p > 0.0)?;
  Some(Price {
    label: v.str("priceText"),
    ..Price::cny(amount)
  })
}

/// Photos and videos the page features, as served (watermarked).
fn media(v: &Value) -> Vec<Media> {
  let mut media: Vec<Media> = v
    .list("advancedPics")
    .iter()
    .filter_map(|p| {
      let mut item = video(p).or_else(|| p.str("url").map(Media::image))?;
      item.alt = p.str("name");
      Some(item)
    })
    .collect();
  if media.is_empty()
    && let Some(cover) = v.str("defaultPic")
  {
    media.push(Media::image(cover));
  }
  media
}

/// The highest-bitrate stream of a featured video.
fn video(p: &Value) -> Option<Media> {
  p.list("skrMediaPlayer.adaptVideo")
    .iter()
    .max_by_key(|s| s.i64("bitrate").unwrap_or_default())
    .and_then(|s| s.str("url"))
    .map(Media::video)
}

fn tags(v: &Value, cache: &Value) -> Vec<String> {
  let mut tags: Vec<String> = ["categoryName", "regionName", "rank.title"]
    .iter()
    .filter_map(|k| v.str(k))
    .collect();
  if let Some(info) = cached(cache, "wxmapi/shop/shopinfo") {
    tags.extend(
      info
        .list("shopInfo.dishTags.dishTags")
        .iter()
        .filter_map(|t| t.as_str().map(str::to_owned).or_else(|| t.str("name"))),
    );
  }
  tags.dedup();
  tags
}

fn extra(post: &mut Post, v: &Value, cache: &Value) {
  let extra = &mut post.extra;
  let mut put = |key: &str, value: Option<Value>| {
    let empty = |x: &Value| x.is_null() || x.as_array().is_some_and(Vec::is_empty);
    if let Some(value) = value.filter(|x| !empty(x)) {
      extra.insert(key.into(), value);
    }
  };
  put("stars", v.f64("shopPower").map(|p| (p / 10.0).into()));
  put("score", v.str("shopPowerRate").map(Value::from));
  put("scores", Some(v.at("scoreTextList").clone()));
  put("avg_price", v.f64("avgPrice").map(Value::from));
  put("address", v.str("address").map(Value::from));
  put("phones", Some(v.at("phoneNos").clone()));
  put("category", v.str("categoryName").map(Value::from));
  put("region", v.str("regionName").map(Value::from));
  put("city_id", v.i64("cityId").map(Value::from));
  put("hours", hours(v).map(Value::from));
  put("status", v.str("bizStatusTime.bizStatus").map(Value::from));
  put("shop_uuid", uuid(v).map(Value::from));
  put("rank", rank(v, cache));
  put(
    "deals",
    cached(cache, "dealshelf/dealfilterlist")
      .and_then(|d| d.i64("msg.totalCount").map(Value::from)),
  );
  put(
    "recommended_dishes",
    cached(cache, "wxmapi/shop/shopinfo").and_then(|d| {
      d.i64("shopInfo.dishTags.recommendDishCount")
        .map(Value::from)
    }),
  );
}

/// The shop's uuid, from its service-info link.
pub fn uuid(v: &Value) -> Option<String> {
  v.str("shopServeInfoUrl")
    .and_then(|u| shops::uuid_in(&u))
    .or_else(|| {
      v.str("shopUuid")
        .filter(|u| !u.chars().all(|c| c.is_ascii_digit()))
    })
}

/// The ranking the shop is on: its title, key and page.
fn rank(v: &Value, cache: &Value) -> Option<Value> {
  let title = v.str("rank.title")?;
  let info = cached(cache, "wxmapi/shop/rankinfo");
  let page = info.and_then(|i| i.str("rankInfo.rankUrl"));
  let key = [page.clone(), v.str("rank.scheme")]
    .into_iter()
    .flatten()
    .find_map(|link| rank_key(&link));
  Some(serde_json::json!({ "title": title, "key": key, "url": page }))
}

fn rank_key(link: &str) -> Option<String> {
  let lower = link.to_ascii_lowercase();
  let at = lower.find("rankkey=")? + "rankkey=".len();
  let key: String = link[at..]
    .chars()
    .take_while(|c| c.is_ascii_alphanumeric())
    .collect();
  (!key.is_empty()).then_some(key)
}
