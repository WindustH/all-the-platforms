//! JD payloads → core model. Items are posts (`kind` `item`) with their
//! price in `price`; shops are users; reviews are comments; hot and
//! related search words are posts of kind `keyword`.

use atp_core::text::{from_beijing, html_to_text};
use atp_core::{Comment, Extra, Media, Metrics, Post, Price, User, Value, ValueExt, json};

use crate::api::{image_url, item_url, search_url, shop_url};

fn insert(extra: &mut Extra, key: &str, value: Option<Value>) {
  if let Some(value) = value.filter(|v| !v.is_null() && v != "" && v != &json!([])) {
    extra.insert(key.into(), value);
  }
}

/// A number in a JSON string or number.
fn number(v: &Value, path: &str) -> Option<f64> {
  let value = v.at(path);
  value
    .as_f64()
    .or_else(|| {
      value
        .as_str()?
        .trim()
        .trim_start_matches(['¥', '￥'])
        .parse()
        .ok()
    })
    .filter(|n| *n > 0.0)
}

/// A shown price, the crossed-out one and the platform's wording.
fn price(amount: Option<f64>, original: Option<f64>, label: Option<String>) -> Option<Price> {
  let mut price = Price::cny(amount?);
  price.original = original.filter(|o| (*o - price.amount).abs() > f64::EPSILON);
  price.label = label.filter(|l| !l.is_empty());
  Some(price)
}

fn shop(id: Option<String>, name: Option<String>) -> Option<User> {
  let name = name.filter(|n| !n.is_empty());
  let id = id.filter(|i| !i.is_empty() && i != "0");
  if id.is_none() && name.is_none() {
    return None;
  }
  Some(User {
    url: id.as_deref().map(shop_url),
    name: name.unwrap_or_default(),
    id: id.unwrap_or_default(),
    ..User::default()
  })
}

/// A search result (`data.wareList[]`) or an item of a followed / history
/// list.
pub fn ware(v: &Value) -> Option<Post> {
  let id = v.first_str(&["skuId", "wareId", "sku", "productId"])?;
  let shown = v.first_str(&["finalPrice.estimatedPrice", "finalPrice.price"]);
  let jd = number(v, "jdPrice")
    .or_else(|| number(v, "price"))
    .or_else(|| number(v, "p"));
  let amount = shown.as_deref().and_then(|s| s.parse().ok()).or(jd);
  let label = shown.and(v.str("finalPrice.title"));
  let floor: Vec<String> = v
    .list("commentSalesFloor")
    .iter()
    .filter_map(|f| f.first_str(&["text", "msg", "desc"]))
    .collect();
  let mut post = Post {
    url: Some(item_url(&id)),
    id,
    kind: "item".into(),
    title: v
      .first_str(&["wareName", "shortName", "skuName", "wname", "name", "title"])
      .map(|t| html_to_text(&t)),
    author: shop(
      v.first_str(&["shopId", "venderId"]),
      v.first_str(&["shopName", "venderName"]),
    ),
    media: v
      .first_str(&["imageurl", "imageUrl", "imgUrl", "imagePath", "skuImg"])
      .map(|u| Media::image(image_url(&u)))
      .into_iter()
      .collect(),
    tags: floor,
    raw: Some(v.clone()),
    ..Post::default()
  };
  post.metrics.comments = v.count("commentCount").or_else(|| v.count("comment"));
  post.price = price(amount, jd.filter(|_| label.is_some()), label);
  let extra = &mut post.extra;
  insert(extra, "selling_points", v.get("sellingPoint").cloned());
  insert(extra, "vender_id", v.str("venderId").map(Value::from));
  insert(extra, "ad", v.bool("isAdv").filter(|a| *a).map(Value::from));
  Some(post)
}

/// The first list of items (objects with a SKU) anywhere in `v`.
pub fn items(v: &Value) -> Vec<Post> {
  let mut level = vec![v];
  for _ in 0..6 {
    let mut next = Vec::new();
    for value in level {
      if let Some(list) = value.as_array()
        && list.iter().any(|e| {
          ["skuId", "wareId", "productId"]
            .iter()
            .any(|k| e.get(k).is_some())
        })
      {
        return list.iter().filter_map(ware).collect();
      }
      match value {
        Value::Object(map) => next.extend(map.values()),
        Value::Array(list) => next.extend(list),
        _ => {}
      }
    }
    level = next;
  }
  Vec::new()
}

/// An item page (`pc_detailpage_wareBusiness`).
pub fn item(v: &Value, sku: &str) -> Option<Post> {
  let title = v.first_str(&[
    "skuHeadVO.skuTitle",
    "wareInfoReadMap.sku_name",
    "wareInfo.wname",
  ])?;
  let shop_info = v.at("itemShopInfo");
  let main = v.first_str(&["mainImageVO.mainImageArea.imageUrl", "pageConfigVO.src"]);
  let mut post = Post {
    id: sku.to_owned(),
    kind: "item".into(),
    title: Some(title),
    url: Some(item_url(sku)),
    author: shop(shop_info.str("shopId"), shop_info.str("shopName")),
    media: images(v, main),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let current = number(v, "price.finalPrice.price").or_else(|| number(v, "price.p"));
  let label = v
    .str("price.finalPrice.priceContent")
    .or_else(|| number(v, "price.finalPrice.price").map(|_| "到手价".to_owned()));
  post.price = price(
    current,
    number(v, "price.op").or_else(|| number(v, "price.p")),
    label,
  );
  post.metrics = Metrics {
    comments: v.first_count(&["commentInfo.commentCount", "commentCount"]),
    ..Metrics::default()
  };
  item_extra(&mut post.extra, v);
  Some(post)
}

fn images(v: &Value, main: Option<String>) -> Vec<Media> {
  let mut urls: Vec<String> = main.into_iter().collect();
  for image in v
    .list("mainImageVO.imageList")
    .iter()
    .chain(v.list("imageInfo.imageList"))
  {
    if let Some(url) = image
      .as_str()
      .map(str::to_owned)
      .or_else(|| image.first_str(&["big", "imgUrl", "imageUrl"]))
    {
      urls.push(url);
    }
  }
  let mut seen = Vec::new();
  urls
    .into_iter()
    .map(|u| image_url(&u))
    .filter(|u| {
      let fresh = !seen.contains(u);
      seen.push(u.clone());
      fresh
    })
    .map(Media::image)
    .collect()
}

fn item_extra(extra: &mut Extra, v: &Value) {
  insert(
    extra,
    "purchase_price",
    number(v, "bestPromotion.purchasePrice").map(Value::from),
  );
  insert(
    extra,
    "brand",
    v.str("wareInfoReadMap.cn_brand").map(Value::from),
  );
  insert(
    extra,
    "vender_id",
    v.str("pageConfigVO.venderId").map(Value::from),
  );
  insert(
    extra,
    "stock",
    v.first_str(&["stockInfo.stockDesc", "stockInfo.promiseResult"])
      .map(|s| json!(html_to_text(&s))),
  );
  let category: Vec<Value> = v
    .list("pageConfigVO.catName")
    .iter()
    .filter(|c| c.as_str().is_some_and(|s| !s.is_empty()))
    .cloned()
    .collect();
  insert(extra, "category", Some(json!(category)));
  let attributes: serde_json::Map<String, Value> = v
    .list("productAttributeVO.attributes")
    .iter()
    .filter_map(|a| Some((a.str("labelName")?, json!(a.str("labelValue")?))))
    .collect();
  if !attributes.is_empty() {
    extra.insert("attributes".into(), Value::Object(attributes));
  }
  insert(extra, "variants", v.get("colorSizeVO").cloned());
}

/// A review (`commentInfoList[]` of the first screen, or a list page's
/// `commentInfo`).
pub fn comment(v: &Value) -> Option<Comment> {
  let id = v.first_str(&["commentId", "guid"])?;
  let text = v.first_str(&["commentData", "tagCommentContent", "content"])?;
  let mut comment = Comment {
    id,
    author: v.str("userNickName").map(|name| User {
      name,
      avatar: v.str("userImgURL").map(|u| image_url(&u)),
      ..User::default()
    }),
    text,
    created_at: v
      .first_str(&["commentDate", "newCommentDate"])
      .and_then(|d| from_beijing(&d)),
    likes: v.first_count(&["praiseCnt", "usefulVoteCount"]),
    reply_count: v.first_count(&["replyCnt", "replyCount"]),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  let pictures: Vec<String> = v
    .list("pictureInfoList")
    .iter()
    .filter_map(|p| p.first_str(&["largePicURL", "largePicUrl", "picURL"]))
    .map(|u| image_url(&u))
    .collect();
  let extra = &mut comment.extra;
  insert(extra, "images", Some(json!(pictures)));
  insert(
    extra,
    "score",
    v.first_count(&["commentScore", "score"]).map(Value::from),
  );
  insert(extra, "variant", v.get("wareAttribute").cloned());
  insert(extra, "bought", v.str("buyCountText").map(Value::from));
  Some(comment)
}

/// A hot search word (`pc_search_hotwords`): mostly promotions.
pub fn hotword(v: &Value) -> Option<Post> {
  let text = v.first_str(&["n", "ext_columns.text", "keyword"])?;
  Some(Post {
    id: v.str("gid").unwrap_or_else(|| text.clone()),
    kind: "keyword".into(),
    url: Some(match v.str("u") {
      Some(link) if link.starts_with("//") => format!("https:{link}"),
      Some(link) => link,
      None => search_url(&text),
    }),
    title: Some(text),
    raw: Some(v.clone()),
    ..Post::default()
  })
}

/// A related search word (`pc_search_relwords`, `relsearch`).
pub fn keyword(v: &Value) -> Option<Post> {
  let text = v.str("keyword")?;
  let search = v.str("searchKey").unwrap_or_else(|| text.clone());
  Some(Post {
    id: text.clone(),
    kind: "keyword".into(),
    url: Some(search_url(&search)),
    title: Some(text),
    media: v
      .str("icon")
      .map(|i| Media::image(image_url(&i)))
      .into_iter()
      .collect(),
    raw: Some(v.clone()),
    ..Post::default()
  })
}

/// A shop's home page: its name in the title, ids in hidden fields.
pub fn shop_page(html: &str, id: &str) -> Option<User> {
  let field = |name: &str| {
    let marker = format!("id=\"{name}\" value=\"");
    let start = html.find(&marker)? + marker.len();
    let end = html[start..].find('"')?;
    Some(html[start..start + end].trim().to_owned()).filter(|v| !v.is_empty())
  };
  let title = html.split("<title>").nth(1)?.split("</title>").next()?;
  let name = html_to_text(
    title
      .trim()
      .trim_end_matches("- 京东")
      .trim_end_matches(" - 京东")
      .trim(),
  );
  let mut user = User {
    id: id.to_owned(),
    name,
    url: Some(shop_url(id)),
    ..User::default()
  };
  let extra = &mut user.extra;
  insert(extra, "vender_id", field("vender_id").map(Value::from));
  insert(
    extra,
    "main_category",
    field("mainCategoryId").map(Value::from),
  );
  Some(user)
}

/// `%XX` escapes decoded (cookie values such as Chinese pins).
pub fn unescape(text: &str) -> String {
  let bytes = text.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    let hex = bytes
      .get(i + 1..i + 3)
      .and_then(|h| std::str::from_utf8(h).ok())
      .and_then(|h| u8::from_str_radix(h, 16).ok());
    match (bytes[i], hex) {
      (b'%', Some(byte)) => {
        out.push(byte);
        i += 3;
      }
      (byte, _) => {
        out.push(byte);
        i += 1;
      }
    }
  }
  String::from_utf8_lossy(&out).into_owned()
}
