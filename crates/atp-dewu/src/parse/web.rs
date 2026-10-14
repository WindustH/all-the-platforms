//! Products of the PC site: pick feeds (`feeds/info`), pick tabs, product
//! details (`detailV3`) and per-size buy prices (`buy-layer`). Amounts are
//! in fen.

use std::collections::BTreeMap;

use atp_core::{Collection, Media, Post, Price, User, Value, ValueExt, json};

use super::{amount, insert, media, minor};
use crate::api::{WWW, media_url, product_url};

/// Fields a per-size price may come in, most specific first.
const PRICE_FIELDS: &[&str] = &["price", "minPrice", "showPrice", "salePrice", "buyPrice"];

/// A product of a pick feed (`data.list[]`).
pub fn pick(v: &Value) -> Option<Post> {
  let spu = v.str("spuId")?;
  let mut post = Post {
    url: Some(product_url(&spu)),
    id: spu,
    kind: "item".into(),
    title: v.str("title"),
    media: image(v.str("logoUrl")),
    price: minor(v, "price").map(Price::cny),
    raw: Some(v.clone()),
    ..Post::default()
  };
  insert(&mut post.extra, "sku_id", v.get("skuId").cloned());
  Some(post)
}

/// A tab of the home page (`data.checkRespDTOList[]`).
pub fn tab(v: &Value) -> Option<Collection> {
  Some(Collection {
    id: v.str("pickId")?,
    kind: "tab".into(),
    name: v.str("name").unwrap_or_default(),
    url: Some(format!("{WWW}/")),
    raw: Some(v.clone()),
    ..Collection::default()
  })
}

fn image(url: Option<String>) -> Vec<Media> {
  url
    .map(|u| media(Media::image(media_url(&u))))
    .into_iter()
    .collect()
}

/// A product page (`detailV3` `data`), without per-size buy prices.
pub fn product(v: &Value, spu: &str) -> Option<Post> {
  let detail = v.at("detail");
  let title = detail.str("title")?;
  let brand = v.at("baseProperties.brandList.0");
  let mut post = Post {
    id: detail.str("spuId").unwrap_or_else(|| spu.to_owned()),
    kind: "item".into(),
    title: Some(title),
    text: detail.str("subTitle"),
    url: Some(product_url(spu)),
    author: brand.str("brandName").map(|name| User {
      id: brand.str("brandId").unwrap_or_default(),
      name,
      ..User::default()
    }),
    media: images(v),
    price: minor(v, "item.price").map(Price::cny),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let params: serde_json::Map<String, Value> = v
    .list("baseProperties.list")
    .iter()
    .filter_map(|p| Some((p.str("key")?, Value::from(p.str("value")?))))
    .collect();
  let extra = &mut post.extra;
  insert(
    extra,
    "article_number",
    detail.str("articleNumber").map(Value::from),
  );
  insert(
    extra,
    "retail_price",
    minor(detail, "authPrice").map(amount),
  );
  insert(
    extra,
    "release_date",
    detail.str("sellDate").map(Value::from),
  );
  insert(extra, "category_id", detail.get("categoryId").cloned());
  insert(extra, "params", Some(Value::Object(params)));
  insert(extra, "sizes", Some(Value::Array(sizes(v))));
  Some(post)
}

/// The gallery; the logo when there is none.
fn images(v: &Value) -> Vec<Media> {
  let mut urls: Vec<String> = Vec::new();
  for image in v.list("image.spuImage.images") {
    if let Some(url) = image.str("url").map(|u| media_url(&u))
      && !urls.contains(&url)
    {
      urls.push(url);
    }
  }
  if urls.is_empty() {
    return image(v.str("detail.logoUrl"));
  }
  urls.into_iter().map(|u| media(Media::image(u))).collect()
}

/// Every SKU with its sale properties (`黑色 42`) and retail price.
fn sizes(v: &Value) -> Vec<Value> {
  let names: BTreeMap<String, String> = v
    .list("saleProperties.list")
    .iter()
    .filter_map(|p| Some((p.str("propertyValueId")?, p.str("value")?)))
    .collect();
  v.list("skus")
    .iter()
    .filter_map(|sku| {
      let id = sku.str("skuId")?;
      let mut properties: Vec<&Value> = sku.list("properties").iter().collect();
      properties.sort_by_key(|p| p.i64("level").unwrap_or_default());
      let label = properties
        .iter()
        .filter_map(|p| names.get(&p.str("propertyValueId")?).cloned())
        .collect::<Vec<_>>()
        .join(" ");
      let mut size = json!({ "sku_id": id, "size": label });
      if let Some(retail) = minor(sku, "authPrice") {
        size["retail_price"] = amount(retail);
      }
      Some(size)
    })
    .collect()
}

/// Lowest buy price per SKU (yuan) anywhere in a `buy-layer` answer:
/// objects with a `skuId` and a price field.
pub fn buy_prices(v: &Value) -> BTreeMap<String, f64> {
  let mut prices = BTreeMap::new();
  collect_prices(v, 0, &mut prices);
  prices
}

fn collect_prices(v: &Value, depth: u8, prices: &mut BTreeMap<String, f64>) {
  if depth > 8 {
    return;
  }
  match v {
    Value::Object(map) => {
      let price = PRICE_FIELDS.iter().find_map(|f| minor(v, f));
      if let (Some(sku), Some(price)) = (v.str("skuId"), price) {
        let slot = prices.entry(sku).or_insert(price);
        *slot = slot.min(price);
      }
      map
        .values()
        .for_each(|child| collect_prices(child, depth + 1, prices));
    }
    Value::Array(items) => items
      .iter()
      .for_each(|child| collect_prices(child, depth + 1, prices)),
    _ => {}
  }
}

/// Put buy prices on a product's sizes; the lowest becomes its price.
pub fn add_buy_prices(post: &mut Post, prices: &BTreeMap<String, f64>) {
  if prices.is_empty() {
    return;
  }
  if let Some(Value::Array(sizes)) = post.extra.get_mut("sizes") {
    for size in sizes.iter_mut() {
      if let Some(price) = size.str("sku_id").and_then(|id| prices.get(&id)) {
        size["price"] = amount(*price);
      }
    }
  }
  let lowest = prices.values().copied().fold(f64::INFINITY, f64::min);
  if lowest.is_finite() {
    post.price = Some(Price::cny(lowest));
  }
}
