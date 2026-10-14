//! Goods cards of the H5 lists (home page, home feed, category tabs,
//! search), hot words and home tabs.

use atp_core::text::html_to_text;
use atp_core::{Collection, Media, Post, Value, ValueExt};

use super::{fen, id, insert, price, shop};
use crate::api::{goods_url, image_url, search_url};

/// A goods card: the object itself or its `data`, `goods_model` or
/// `item_data.goods_model`.
pub fn goods(v: &Value) -> Option<Post> {
  let v = ["item_data.goods_model", "goods_model", "data"]
    .iter()
    .map(|p| v.at(p))
    .find(|g| g.get("goods_id").is_some())
    .unwrap_or(v);
  let id = id(v, &["goods_id", "goodsID"])?;
  let mut post = Post {
    url: Some(goods_url(&id)),
    id,
    kind: "item".into(),
    title: v
      .first_str(&["goods_name", "short_name", "goodsName"])
      .map(|t| html_to_text(&t)),
    author: shop(v.str("mall_id"), v.str("mall_name"), None),
    media: v
      .first_str(&["hd_thumb_url", "hd_url", "thumb_url", "image_url"])
      .map(|u| Media::image(image_url(&u)))
      .into_iter()
      .collect(),
    tags: tags(v),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let group = fen(v, "group.price").or_else(|| fen(v, "price"));
  let single = fen(v, "normal_price");
  post.price = price(
    group.or(single),
    fen(v, "market_price"),
    v.str("price_info").or_else(|| v.str("price_prefix")),
  );
  let extra = &mut post.extra;
  insert(
    extra,
    "single_price",
    single.filter(|_| group.is_some()).map(Value::from),
  );
  insert(
    extra,
    "group_size",
    v.get("group").and_then(|g| g.get("customer_num")).cloned(),
  );
  insert(extra, "sales_tip", v.str("sales_tip").map(Value::from));
  insert(
    extra,
    "ad",
    v.get("ad")
      .filter(|a| a.is_object())
      .map(|_| Value::from(true)),
  );
  Some(post)
}

/// The goods cards of an answer whose layout is not pinned down: the
/// first list (breadth first, a few levels deep) holding goods.
pub fn goods_list(v: &Value) -> Vec<Post> {
  let is_goods = |e: &Value| {
    [
      "goods_id",
      "data.goods_id",
      "goods_model.goods_id",
      "item_data.goods_model.goods_id",
    ]
    .iter()
    .any(|p| !e.at(p).is_null())
  };
  let mut level = vec![v];
  for _ in 0..5 {
    let mut next = Vec::new();
    for value in level {
      match value {
        Value::Array(list) if list.iter().any(is_goods) => {
          return list.iter().filter_map(goods).collect();
        }
        Value::Array(list) => next.extend(list),
        Value::Object(map) => next.extend(map.values()),
        _ => {}
      }
    }
    level = next;
  }
  Vec::new()
}

/// Labels on a card (`tag_list[].text`, `prop_tag_list[].text`).
fn tags(v: &Value) -> Vec<String> {
  v.list("tag_list")
    .iter()
    .chain(v.list("prop_tag_list"))
    .filter_map(|t| t.first_str(&["text", "tag_text"]))
    .collect()
}

/// A hot search word of the home page (`hotQueries[]`, `shadeQuery`).
pub fn hot_word(v: &Value) -> Option<Post> {
  let text = v.first_str(&["text", "query", "keyword"])?;
  Some(Post {
    id: text.clone(),
    kind: "keyword".into(),
    url: Some(search_url(&text)),
    title: Some(text),
    tags: v
      .list("tags")
      .iter()
      .filter_map(|t| t.as_str().map(str::to_owned).or_else(|| t.str("text")))
      .collect(),
    raw: Some(v.clone()),
    ..Post::default()
  })
}

/// A home page tab (`operations[]`): a category with its `opt_id`.
pub fn tab(v: &Value) -> Option<Collection> {
  let name = v.str("tabName")?;
  Some(Collection {
    id: v.str("optID").unwrap_or_else(|| "0".into()),
    kind: "category".into(),
    name,
    raw: Some(v.clone()),
    ..Collection::default()
  })
}
