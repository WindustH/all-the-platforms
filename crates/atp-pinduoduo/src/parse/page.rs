//! Logged-in pages and their calls: the goods page (`initDataObj.goods`,
//! camelCase with prices in yuan, or the raw snake_case goods with prices
//! in fen), shops and reviews.

use atp_core::text::{from_secs, html_to_text};
use atp_core::{Comment, Extra, Media, Post, User, Value, ValueExt, json};

use super::{fen, id, images, insert, price, shop, sold, yuan};
use crate::api::{goods_url, image_url, mall_url};

/// The goods of a goods page store, with its shop.
pub fn detail(store: &Value, goods_id: &str) -> Option<Post> {
  let data = store.at("initDataObj");
  let g = data.at("goods");
  let title = g.first_str(&["goodsName", "goods_name"])?;
  let mut post = Post {
    id: id(g, &["goodsID", "goods_id"]).unwrap_or_else(|| goods_id.to_owned()),
    kind: "item".into(),
    title: Some(html_to_text(&title)),
    text: g.first_str(&["goodsDesc", "goods_desc"]),
    url: Some(goods_url(goods_id)),
    author: detail_shop(store),
    media: gallery(g),
    raw: Some(store.clone()),
    ..Post::default()
  };
  let group = yuan(g, "minOnSaleGroupPrice")
    .or_else(|| yuan(g, "minGroupPrice"))
    .or_else(|| fen(g, "min_on_sale_group_price"))
    .or_else(|| fen(g, "min_group_price"));
  let single = yuan(g, "minOnSaleNormalPrice").or_else(|| fen(g, "min_normal_price"));
  let line = yuan(g, "linePrice").or_else(|| fen(g, "line_price"));
  post.price = price(group.or(single), line, None);
  let tip = g.first_str(&["sideSalesTip", "side_sales_tip", "bottomSalesTip"]);
  if let Some(n) = tip.as_deref().and_then(sold) {
    post.metrics.other.insert("sold".into(), n);
  }
  let extra = &mut post.extra;
  insert(extra, "single_price", single.map(Value::from));
  insert(extra, "sales_tip", tip.map(Value::from));
  insert(extra, "skus", Some(skus(g)));
  insert(
    extra,
    "properties",
    g.get("goodsProperty")
      .or_else(|| g.get("goods_property"))
      .cloned(),
  );
  Some(post)
}

/// Gallery images first, then the videos (`videoGallery[].videoUrl`; in
/// a raw `gallery`, entries pointing at videos).
fn gallery(g: &Value) -> Vec<Media> {
  let is_video = |u: &str| u.contains(".mp4") || u.contains("/video/");
  let shown = g.list("topGallery").iter().chain(g.list("viewImageData"));
  let mut media = images(shown, &["url"]);
  if media.is_empty() {
    let raw = g
      .list("gallery")
      .iter()
      .filter(|i| !i.str("url").is_some_and(|u| is_video(&u)));
    media = images(raw, &["url"]);
  }
  let videos = g
    .list("videoGallery")
    .iter()
    .filter_map(|v| v.first_str(&["videoUrl", "video_url"]))
    .chain(g.list("gallery").iter().filter_map(|i| {
      i.str("video_url")
        .or_else(|| i.str("url").filter(|u| is_video(u)))
    }))
    .chain(g.first_str(&["video.url", "video.play_url"]));
  for url in videos {
    let url = image_url(&url);
    if !media.iter().any(|m| m.url == url) {
      media.push(Media::video(url));
    }
  }
  media
}

/// SKUs: id, spec values, group and single price, stock.
fn skus(g: &Value) -> Value {
  let list = if g.list("skus").is_empty() {
    g.list("sku")
  } else {
    g.list("skus")
  };
  let skus: Vec<Value> = list
    .iter()
    .map(|s| {
      let specs: Vec<String> = s
        .list("specs")
        .iter()
        .filter_map(|p| p.first_str(&["spec_value", "specValue"]))
        .collect();
      json!({
        "id": s.first_str(&["skuID", "sku_id"]),
        "spec": specs.join(" / "),
        "price": yuan(s, "groupPrice").or_else(|| fen(s, "group_price")),
        "single_price": yuan(s, "normalPrice").or_else(|| fen(s, "normal_price")),
        "stock": s.get("quantity"),
        "image": s.first_str(&["thumbUrl", "thumb_url"]).map(|u| image_url(&u)),
      })
    })
    .collect();
  Value::Array(skus)
}

fn detail_shop(store: &Value) -> Option<User> {
  let m = [
    store.at("initDataObj.mall"),
    store.at("mallDetailInfo"),
    store.at("initDataObj.goods"),
  ]
  .into_iter()
  .find(|m| {
    m.first_str(&["mallName", "mall_name", "mallID", "mall_id"])
      .is_some()
  })?;
  shop(
    id(m, &["mallID", "mall_id"]),
    m.first_str(&["mallName", "mall_name"]),
    m.first_str(&["logo", "mallLogo", "mall_logo"]),
  )
}

/// A shop from its page store or its info call: name, logo, counts.
pub fn mall(v: &Value, mall_id: &str) -> Option<User> {
  let m = [
    v.at("mall"),
    v.at("mallInfo"),
    v.at("mall_info"),
    v.at("initDataObj.mall"),
    v.at("result"),
    v,
  ]
  .into_iter()
  .find(|m| m.first_str(&["mall_name", "mallName"]).is_some())?;
  let mut user = User {
    id: mall_id.to_owned(),
    name: m.first_str(&["mall_name", "mallName"])?,
    url: Some(mall_url(mall_id)),
    avatar: m
      .first_str(&["logo", "mall_logo", "mallLogo"])
      .map(|u| image_url(&u)),
    bio: m.first_str(&["mall_desc", "mallDesc"]),
    raw: Some(v.clone()),
    ..User::default()
  };
  user.stats.posts = m.first_count(&["goods_num", "goodsNum"]);
  user.stats.followers = m.first_count(&["fav_num", "favNum", "fans_num"]);
  insert(
    &mut user.extra,
    "sales_tip",
    m.first_str(&["sales_tip", "salesTip", "mall_sales"])
      .map(Value::from),
  );
  insert(
    &mut user.extra,
    "scores",
    m.get("dsr")
      .or_else(|| m.get("mall_rating_text_list"))
      .cloned(),
  );
  Some(user)
}

/// A review (`/reviews/<goods>/list` → `data[]`).
pub fn review(v: &Value) -> Option<Comment> {
  let id = id(v, &["review_id", "reviewId"])?;
  let text = v.first_str(&["comment", "content"]).unwrap_or_default();
  let mut comment = Comment {
    id,
    author: v.first_str(&["name", "nickname"]).map(|name| User {
      name,
      avatar: v.str("avatar").map(|u| image_url(&u)),
      ..User::default()
    }),
    text,
    created_at: v.i64("time").and_then(from_secs),
    likes: v.first_count(&["favor_count", "like_count"]),
    reply_count: v.first_count(&["reply_count"]),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  review_extra(&mut comment.extra, v);
  Some(comment)
}

fn review_extra(extra: &mut Extra, v: &Value) {
  let pictures = images(
    v.list("pictures").iter().chain(v.list("append.pictures")),
    &["url"],
  );
  let pictures: Vec<String> = pictures.into_iter().map(|m| m.url).collect();
  insert(extra, "images", Some(json!(pictures)));
  let video = v
    .first_str(&["video.url", "video.video_url"])
    .map(|u| image_url(&u));
  insert(extra, "video", video.map(Value::from));
  insert(
    extra,
    "stars",
    v.get("stars").or_else(|| v.get("desc_score")).cloned(),
  );
  let specs: Vec<String> = v
    .list("specs")
    .iter()
    .filter_map(|s| s.first_str(&["spec_value", "value"]))
    .collect();
  insert(
    extra,
    "variant",
    (!specs.is_empty()).then(|| json!(specs.join(" / "))),
  );
  insert(extra, "append", v.str("append.comment").map(Value::from));
}
