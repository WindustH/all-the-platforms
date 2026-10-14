//! Open Platform answers: the catalogue entry of an article number
//! (`by-article-number`), per-size reference prices (`batchPrice`) and
//! 30-day sales (`by-global-sku` with statistics). Prices come in the
//! currency's minor unit.

use std::collections::BTreeMap;

use atp_core::{Post, Price, Value, ValueExt, json};

use super::{amount, insert, minor};
use crate::api::product_url;

/// Price fields of `batchPrice`, as atp names them.
const PRICES: &[(&str, &str)] = &[
  ("globalMinPrice", "global_min"),
  ("asiaMinPrice", "asia_min"),
  ("localMinPrice", "local_min"),
  ("usMinPrice", "us_min"),
  ("fen95ReferencePrice", "fen95_reference"),
  ("otherPlatformMinPrice", "other_platform_min"),
];

/// A product of the catalogue (`data[]`), its sizes in `extra.sizes`
/// with their `global_sku_id`.
pub fn catalogue(v: &Value) -> Option<Post> {
  let spu = v.at("spuInfo");
  let dw = spu.str("dwSpuId");
  let global = v.str("globalSpuId");
  let mut post = Post {
    id: dw.clone().or_else(|| global.clone())?,
    kind: "item".into(),
    title: spu.str("title"),
    url: dw.as_deref().map(product_url),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let sizes: Vec<Value> = v.list("skuInfoList").iter().filter_map(size).collect();
  let extra = &mut post.extra;
  insert(
    extra,
    "article_number",
    spu.str("articleNumber").map(Value::from),
  );
  insert(extra, "global_spu_id", global.map(Value::from));
  insert(
    extra,
    "category",
    spu.str("level1CategoryName").map(Value::from),
  );
  insert(extra, "sizes", Some(Value::Array(sizes)));
  Some(post)
}

/// One SKU: its size (`尺码` / `Size`, else the last sale property).
fn size(sku: &Value) -> Option<Value> {
  let id = sku.str("globalSkuId")?;
  let properties: Vec<(String, String)> = sku
    .list("regionSalePvInfoList")
    .iter()
    .filter_map(|p| Some((p.str("name")?, p.str("value")?)))
    .collect();
  let label = ["尺码", "Size", "size"]
    .iter()
    .find_map(|n| properties.iter().find(|(k, _)| k == n))
    .or(properties.last())
    .map(|(_, v)| v.clone());
  let mut size = json!({ "global_sku_id": id, "size": label });
  if let Some(sku_id) = sku.str("dwSkuId") {
    size["sku_id"] = sku_id.into();
  }
  Some(size)
}

/// The `global_sku_id`s of a catalogue product.
pub fn global_skus(post: &Post) -> Vec<String> {
  post
    .extra
    .get("sizes")
    .map(|s| s.list(""))
    .unwrap_or_default()
    .iter()
    .filter_map(|s| s.str("global_sku_id"))
    .collect()
}

/// Apply `fill` to every size whose `global_sku_id` has a row in `rows`.
fn each_size(post: &mut Post, rows: &BTreeMap<String, &Value>, fill: impl Fn(&mut Value, &Value)) {
  if let Some(Value::Array(sizes)) = post.extra.get_mut("sizes") {
    for size in sizes.iter_mut() {
      if let Some(row) = size.str("global_sku_id").and_then(|id| rows.get(&id)) {
        fill(size, row);
      }
    }
  }
}

/// Put `batchPrice` rows on the sizes; the lowest global minimum becomes
/// the price, in `currency`.
pub fn add_prices(post: &mut Post, rows: &[Value], currency: &str) {
  let rows: BTreeMap<String, &Value> = rows
    .iter()
    .filter_map(|r| Some((r.str("globalSkuId")?, r)))
    .collect();
  each_size(post, &rows, |size, row| {
    for (field, name) in PRICES {
      if let Some(n) = minor(row, field) {
        size[*name] = amount(n);
      }
    }
    if let Some(n) = minor(row, "leakInfos.0.leakPrice") {
      size["leak_price"] = amount(n);
      size["leak_region"] = row.at("leakInfos.0.buyerRegion").clone();
    }
  });
  let lowest = rows
    .values()
    .filter_map(|r| minor(r, "globalMinPrice"))
    .fold(f64::INFINITY, f64::min);
  if lowest.is_finite() {
    post.price = Some(Price {
      amount: lowest,
      currency: currency.to_owned(),
      label: Some("global min".into()),
      ..Price::default()
    });
  }
  insert(&mut post.extra, "currency", Some(currency.into()));
}

/// Put 30-day sales (`data[].skuInfoList[].commoditySales`) on the
/// sizes; their sum goes to `metrics.sales_30d`.
pub fn add_sales(post: &mut Post, data: &Value) {
  let skus: Vec<&Value> = data
    .as_array()
    .map(Vec::as_slice)
    .unwrap_or_default()
    .iter()
    .flat_map(|row| row.list("skuInfoList"))
    .collect();
  let rows: BTreeMap<String, &Value> = skus
    .iter()
    .filter_map(|s| Some((s.str("globalSkuId")?, s.at("commoditySales"))))
    .collect();
  each_size(post, &rows, |size, sales| {
    for (field, name) in [
      ("globalSoldNum30", "sold_30d"),
      ("localSoldNum30", "local_sold_30d"),
      ("globalMonthToMonthRatio", "month_on_month"),
      ("localMonthToMonthRatio", "local_month_on_month"),
    ] {
      if let Some(value) = sales.get(field).filter(|v| !v.is_null()) {
        size[name] = value.clone();
      }
    }
  });
  let total: u64 = rows.values().filter_map(|s| s.u64("globalSoldNum30")).sum();
  post.metrics.other.insert("sales_30d".into(), total);
}
