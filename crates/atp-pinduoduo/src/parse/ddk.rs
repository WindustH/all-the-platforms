//! 多多进宝 goods (`goods_list[]`, `goods_details[]`, recommendation
//! `list[]`) and categories, with the field names of the open API.

use atp_core::{Collection, Media, Post, Value, ValueExt, json};

use super::{fen, id, images, insert, price, shop, sold};
use crate::api::{goods_url, image_url};

/// A DDK goods: `goods_sign` is its id (raw goods ids are retired there);
/// the link is the goods page when a goods id still comes along. The price
/// is the group price after the coupon (`券后`) when there is one.
pub fn ddk_goods(v: &Value) -> Option<Post> {
  let sign = v.str("goods_sign")?;
  let goods_id = id(v, &["goods_id"]);
  let group = fen(v, "min_group_price");
  let coupon = fen(v, "coupon_discount").filter(|_| v.bool("has_coupon") != Some(false));
  let amount = match (group, coupon) {
    (Some(g), Some(c)) if c < g => Some(g - c),
    _ => group,
  };
  let mut media: Vec<Media> = v
    .first_str(&["goods_image_url", "goods_thumbnail_url"])
    .map(|u| Media::image(image_url(&u)))
    .into_iter()
    .collect();
  for image in images(v.list("goods_gallery_urls"), &[]) {
    if !media.iter().any(|m| m.url == image.url) {
      media.push(image);
    }
  }
  media.extend(
    v.list("video_urls")
      .iter()
      .filter_map(|u| u.as_str())
      .map(Media::video),
  );
  let mut post = Post {
    id: sign,
    kind: "item".into(),
    title: v.str("goods_name"),
    text: v.str("goods_desc"),
    url: goods_id.as_deref().map(goods_url),
    author: shop(v.str("mall_id"), v.str("mall_name"), None),
    media,
    tags: tags(v),
    price: price(
      amount,
      group.filter(|_| coupon.is_some()),
      coupon.map(|_| "券后".into()),
    ),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(n) = v.str("sales_tip").and_then(|t| sold(&t)) {
    post.metrics.other.insert("sold".into(), n);
  }
  ddk_extra(&mut post, v, goods_id);
  Some(post)
}

fn tags(v: &Value) -> Vec<String> {
  let mut tags: Vec<String> = v
    .list("unified_tags")
    .iter()
    .filter_map(|t| t.as_str().map(str::to_owned))
    .collect();
  tags.extend(
    ["opt_name", "category_name", "brand_name"]
      .iter()
      .filter_map(|k| v.str(k)),
  );
  tags.dedup();
  tags
}

fn ddk_extra(post: &mut Post, v: &Value, goods_id: Option<String>) {
  let extra = &mut post.extra;
  insert(extra, "goods_id", goods_id.map(Value::from));
  insert(
    extra,
    "single_price",
    fen(v, "min_normal_price").map(Value::from),
  );
  insert(extra, "coupon", fen(v, "coupon_discount").map(Value::from));
  insert(extra, "sales_tip", v.str("sales_tip").map(Value::from));
  let scores: serde_json::Map<String, Value> = [
    ("description", "desc_txt"),
    ("service", "serv_txt"),
    ("logistics", "lgst_txt"),
  ]
  .iter()
  .filter_map(|(name, key)| Some(((*name).to_owned(), json!(v.str(key)?))))
  .collect();
  if !scores.is_empty() {
    extra.insert("shop_scores".into(), Value::Object(scores));
  }
  insert(extra, "search_id", v.str("search_id").map(Value::from));
  insert(extra, "promotion_rate", v.get("promotion_rate").cloned());
}

/// A category of `pdd.goods.opt.get` or `pdd.goods.cats.get`.
pub fn category(v: &Value) -> Option<Collection> {
  let id = id(v, &["opt_id", "cat_id"])?;
  Some(Collection {
    id,
    kind: "category".into(),
    name: v.first_str(&["opt_name", "cat_name"]).unwrap_or_default(),
    raw: Some(v.clone()),
    ..Collection::default()
  })
}
