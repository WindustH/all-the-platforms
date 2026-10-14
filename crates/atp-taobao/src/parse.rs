//! Taobao payloads → core model. Items are posts (`kind` `item`) with their
//! price in `extra.price` (yuan, as shown); sellers are users named after
//! their shop; reviews are comments.

use std::sync::LazyLock;

use atp_alibaba::text::{beijing, https, price};
use atp_core::text::html_to_text;
use atp_core::{Comment, Media, Post, User, Value, ValueExt, json};
use regex::Regex;

use crate::api::item_url;

static USER_ID: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#""userId"\s*:\s*"?(\d+)"#).expect("valid"));
static ENCRYPT_UID: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r#"(?:data-encryptuid="|"encryptUid"\s*:\s*")([^"]+)""#).expect("valid")
});
static SELLER_NICK: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#""sellerNick"\s*:\s*"((?:[^"\\]|\\.)*)""#).expect("valid"));
static SHOP_NAME: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#""shopName"\s*:\s*"((?:[^"\\]|\\.)*)""#).expect("valid"));

/// A search result (`itemsArray[]`).
pub fn search_item(v: &Value) -> Option<Post> {
  let id = v
    .first_str(&["item_id", "nid", "itemId"])
    .filter(|id| id != "0")?;
  let title = v
    .first_str(&["title", "raw_title"])
    .map(|t| html_to_text(&t));
  let shop = v.first_str(&["shopInfo.title", "nick", "shopName"]);
  let mut post = Post {
    url: Some(item_url(&id)),
    id,
    kind: "item".into(),
    title,
    author: shop.map(|name| User {
      name,
      handle: v.str("nick"),
      ..User::default()
    }),
    media: v
      .first_str(&["pic_path", "pic_url", "img"])
      .map(|u| Media::image(https(u)))
      .into_iter()
      .collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let extra = &mut post.extra;
  for (key, value) in [
    ("price", price(v.first_str(&["price", "view_price"]))),
    (
      "sales",
      v.first_str(&["realSales", "view_sales"]).map(Value::from),
    ),
    (
      "location",
      v.first_str(&["procity", "item_loc"]).map(Value::from),
    ),
    (
      "tmall",
      v.bool("shopInfo.isTmall").filter(|t| *t).map(Value::from),
    ),
  ] {
    if let Some(value) = value {
      extra.insert(key.into(), value);
    }
  }
  Some(post)
}

/// An item (`mtop.taobao.pcdetail.data.get`).
pub fn item(data: &Value, id: &str) -> Option<Post> {
  let it = data.at("item");
  let title = it.str("title")?;
  let seller = data.at("seller");
  let author = seller.str("sellerId").map(|seller_id| User {
    id: seller_id,
    name: seller
      .first_str(&["shopName", "sellerNick"])
      .unwrap_or_default(),
    handle: seller.str("sellerNick"),
    url: seller.str("shopUrl").map(https).or_else(|| {
      seller
        .str("shopId")
        .map(|s| format!("https://shop{s}.taobao.com"))
    }),
    ..User::default()
  });
  let mut post = Post {
    id: it.str("itemId").unwrap_or_else(|| id.to_owned()),
    kind: "item".into(),
    title: Some(title),
    text: it.str("subtitle"),
    url: Some(item_url(id)),
    author,
    media: it
      .list("images")
      .iter()
      .filter_map(Value::as_str)
      .map(|u| Media::image(https(u.to_owned())))
      .collect(),
    raw: Some(data.clone()),
    ..Post::default()
  };
  let prices = data.at("skuCore.sku2info.0.price");
  let extra = &mut post.extra;
  if let Some(p) = price(prices.str("priceText")) {
    extra.insert("price".into(), p);
  }
  if let Some(sales) = data.first_str(&["item.vagueSellCount", "item.sellCount", "vagueSellCount"])
  {
    extra.insert("sales".into(), json!(sales));
  }
  if let Some(shop) = seller.str("shopId") {
    extra.insert("shop_id".into(), json!(shop));
  }
  let skus = data.list("skuBase.skus").len();
  if skus > 0 {
    extra.insert("skus".into(), json!(skus));
  }
  Some(post)
}

/// A review (`rateList[]`).
pub fn review(v: &Value) -> Option<Comment> {
  let id = v.first_str(&["id", "rateId"])?;
  let mut comment = Comment {
    id,
    author: v
      .first_str(&["reduceUserNick", "displayUserNick", "userNick"])
      .map(|name| User {
        name,
        ..User::default()
      }),
    text: v.first_str(&["feedback", "content"]).unwrap_or_default(),
    created_at: v
      .first_str(&["feedbackDate", "createTime"])
      .and_then(|d| beijing(&d)),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  let images: Vec<String> = v
    .list("feedPicList")
    .iter()
    .chain(v.list("feedPicPathList"))
    .filter_map(|p| p.as_str().map(str::to_owned).or_else(|| p.str("url")))
    .map(https)
    .collect();
  let extra = &mut comment.extra;
  if !images.is_empty() {
    extra.insert("images".into(), json!(images));
  }
  if let Some(sku) = v.str("skuValueStr") {
    extra.insert("sku".into(), json!(sku));
  }
  if let Some(more) = v.first_str(&["appendedFeed.appendedFeedback", "appendComment.content"]) {
    extra.insert("appended".into(), json!(more));
  }
  Some(comment)
}

/// The seller named in an item page: user id, the encrypted id chats are
/// opened with, nick and shop.
pub struct Seller {
  pub id: String,
  pub encrypted: String,
  pub user: User,
}

pub fn seller(html: &str) -> Option<Seller> {
  let id = USER_ID.captures(html)?[1].to_owned();
  let encrypted = ENCRYPT_UID.captures(html)?[1].to_owned();
  let unquote = |re: &Regex| {
    re.captures(html)
      .and_then(|m| serde_json::from_str::<String>(&format!("\"{}\"", &m[1])).ok())
  };
  let nick = unquote(&SELLER_NICK);
  let user = User {
    id: id.clone(),
    name: unquote(&SHOP_NAME)
      .or_else(|| nick.clone())
      .unwrap_or_default(),
    handle: nick,
    ..User::default()
  };
  Some(Seller {
    id,
    encrypted,
    user,
  })
}

/// A decoded push in the shape of a history entry, or `None` for pushes
/// that are not messages. Fields are numbered: `1.1.1` sender, `1.2` chat,
/// `1.3` id, `1.5` time, `1.6` content (`1` type, `2.1` text, `3` custom),
/// `1.10.sender_nick`.
pub fn push(decoded: &Value) -> Option<Value> {
  let m = decoded.at("1");
  let content = m.at("6");
  Some(json!({"message": {
    "messageId": m.str("3")?,
    "cid": m.str("2")?,
    "createAt": m.i64("5"),
    "sender": {"uid": m.str("1.1")?},
    "content": {
      "contentType": content.at("1"),
      "text": {"content": content.at("2.1")},
      "custom": {"type": content.at("3.4"), "data": content.at("3.5"), "summary": content.at("3.2")},
    },
    "extension": {"sender_nick": m.at("10.sender_nick")},
  }}))
}
