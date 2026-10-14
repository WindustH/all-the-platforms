//! Xianyu payloads → core model. Items are posts (`kind` `item`) with their
//! price in `extra.price` (yuan, as shown), users are sellers and buyers.

use atp_alibaba::text::{beijing, https, price};
use atp_core::text::{from_unix, parse_count};
use atp_core::{Comment, Extra, Media, Metrics, Post, User, UserStats, Value, ValueExt, json};

use crate::api::{item_url, user_url};

fn image(url: Option<String>, width: Option<u64>, height: Option<u64>) -> Option<Media> {
  let mut media = Media::image(https(url?));
  media.width = width.map(|w| w as u32);
  media.height = height.map(|h| h as u32);
  Some(media)
}

fn insert(extra: &mut Extra, key: &str, value: Option<Value>) {
  if let Some(value) = value.filter(|v| !v.is_null()) {
    extra.insert(key.into(), value);
  }
}

/// `itemStatus`: 0 on sale, 1 sold, anything else off the shelf.
fn status(v: &Value) -> Option<Value> {
  Some(json!(match v.i64("itemStatus")? {
    0 => "on_sale",
    1 => "sold",
    _ => "off_shelf",
  }))
}

fn time(v: &Value, path: &str) -> Option<jiff::Timestamp> {
  v.i64(path).and_then(from_unix)
}

/// A search result (`resultList[]`).
pub fn search_item(v: &Value) -> Option<Post> {
  let main = v.at("data.item.main");
  let ex = main.at("exContent");
  let id = ex.str("itemId")?;
  let shown: String = ex
    .list("price")
    .iter()
    .filter_map(|p| p.str("text"))
    .collect();
  let tags = ["r1", "r2", "r3", "r4"]
    .iter()
    .flat_map(|r| ex.list(&format!("fishTags.{r}.tagList")))
    .filter_map(|t| t.str("data.content"))
    .collect();
  let mut post = Post {
    url: Some(item_url(&id)),
    id,
    kind: "item".into(),
    title: ex.str("title"),
    author: ex.str("userNickName").map(|name| User {
      name,
      ..User::default()
    }),
    created_at: time(main, "clickParam.args.publishTime"),
    media: image(ex.str("picUrl"), None, None).into_iter().collect(),
    tags,
    raw: Some(v.clone()),
    ..Post::default()
  };
  post.price = price(
    Some(shown).filter(|s| !s.is_empty()),
    ex.str("originalPrice"),
  );
  insert(&mut post.extra, "location", ex.str("area").map(Value::from));
  Some(post)
}

/// An item page (`itemDO` and `sellerDO`).
pub fn item(data: &Value) -> Option<Post> {
  let it = data.at("itemDO");
  let seller = data.at("sellerDO");
  let id = it.str("itemId")?;
  let mut media: Vec<Media> = it
    .list("imageInfos")
    .iter()
    .filter_map(|i| image(i.str("url"), i.u64("widthSize"), i.u64("heightSize")))
    .collect();
  if let Some(video) = it.first_str(&["videoPlayInfo.videoUrl", "videoUrl"]) {
    media.insert(0, Media::video(https(video)));
  }
  let mut metrics = Metrics {
    views: it.count("browseCnt"),
    favorites: it.count("collectCnt"),
    ..Metrics::default()
  };
  if let Some(wants) = it.count("wantCnt") {
    metrics.other.insert("wants".into(), wants);
  }
  let mut post = Post {
    url: Some(item_url(&id)),
    id,
    kind: "item".into(),
    title: it.str("title"),
    text: it.str("desc"),
    author: seller_user(seller, it),
    created_at: time(it, "gmtCreate"),
    metrics,
    media,
    raw: Some(data.clone()),
    ..Post::default()
  };
  post.price = price(
    it.first_str(&["soldPrice", "minPrice"]),
    it.str("originalPrice"),
  );
  let extra = &mut post.extra;
  insert(extra, "status", status(it));
  insert(
    extra,
    "location",
    seller.first_str(&["publishCity", "city"]).map(Value::from),
  );
  let properties: serde_json::Map<String, Value> = it
    .list("itemLabelExtList")
    .iter()
    .filter_map(|l| Some((l.str("propertyText")?, json!(l.str("text")?))))
    .collect();
  if !properties.is_empty() {
    extra.insert("properties".into(), Value::Object(properties));
  }
  Some(post)
}

fn seller_user(seller: &Value, item: &Value) -> Option<User> {
  let id = seller.str("sellerId").or_else(|| item.str("sellerId"))?;
  Some(User {
    url: Some(user_url(&id)),
    name: seller
      .first_str(&["nick", "uniqueName"])
      .unwrap_or_default(),
    avatar: seller.str("portraitUrl").map(https),
    location: seller.str("city"),
    bio: seller.str("signature"),
    id,
    ..User::default()
  })
}

/// A card of the home feed, a user's items or the favorites.
pub fn card(v: &Value) -> Option<Post> {
  let d = if v.at("cardData").is_object() {
    v.at("cardData")
  } else {
    v
  };
  let id = d.first_str(&["itemId", "id"])?;
  let author = d
    .first_str(&["user.userNick", "userNick"])
    .map(|name| User {
      id: d.str("userId").unwrap_or_default(),
      url: d.str("userId").map(|u| user_url(&u)),
      avatar: d.first_str(&["user.avatar", "userAvatar"]).map(https),
      name,
      ..User::default()
    });
  let picture = d.first_str(&["mainPicInfo.url", "picInfo.picUrl", "picUrl"]);
  let mut post = Post {
    url: Some(item_url(&id)),
    id,
    kind: "item".into(),
    title: d.first_str(&["titleSummary.text", "title"]),
    author,
    created_at: time(d, "favorTime").or_else(|| d.str("favorTime").and_then(|t| beijing(&t))),
    media: image(picture, None, None).into_iter().collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  post.metrics.views = d.count("browseCount");
  post.price = price(
    d.first_str(&["priceInfo.price", "price"]),
    d.first_str(&["priceInfo.oriPrice", "originalPrice"]),
  );
  let extra = &mut post.extra;
  insert(extra, "status", status(d));
  insert(
    extra,
    "location",
    d.first_str(&["city", "area"]).map(Value::from),
  );
  Some(post)
}

/// A rating a user received (`cardList[].cardData`): `rate` 1 good, 0
/// neutral, -1 bad; the tag says whether a buyer or a seller gave it.
pub fn rating(v: &Value) -> Option<Comment> {
  let d = v.at("cardData");
  let id = d.str("rateId")?;
  let mut comment = Comment {
    id,
    author: d.str("raterUserNick").map(|name| User {
      name,
      ..User::default()
    }),
    text: d.str("feedback").unwrap_or_default(),
    created_at: time(d, "gmtCreate").or_else(|| d.str("gmtCreate").and_then(|t| beijing(&t))),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  let rate = match d.i64("rate") {
    Some(1) => Some("good"),
    Some(0) => Some("neutral"),
    Some(-1) => Some("bad"),
    _ => None,
  };
  let images: Vec<String> = d
    .list("pictCdnUrlList")
    .iter()
    .filter_map(|p| p.as_str().map(|u| https(u.to_owned())))
    .collect();
  let extra = &mut comment.extra;
  insert(extra, "rate", rate.map(Value::from));
  insert(extra, "from", d.str("rateTagList.0.text").map(Value::from));
  if !images.is_empty() {
    extra.insert("images".into(), json!(images));
  }
  Some(comment)
}

/// A profile (`mtop.idle.web.user.page.head`).
pub fn user(data: &Value, id: &str) -> User {
  let module = data.at("module");
  let base = module.at("base");
  let count = |path: &str| module.str(path).and_then(|s| parse_count(&s));
  let mut stats = UserStats {
    followers: count("social.followers"),
    following: count("social.following"),
    posts: count("tabs.item.number"),
    ..UserStats::default()
  };
  if let Some(ratings) = count("tabs.rate.number") {
    stats.other.insert("ratings".into(), ratings);
  }
  let mut user = User {
    id: id.to_owned(),
    name: base.str("displayName").unwrap_or_default(),
    url: Some(user_url(id)),
    avatar: base.str("avatar.avatar").map(https),
    bio: base.str("introduction"),
    location: base.str("ipLocation"),
    stats,
    followed: module.i64("social.followStatus").map(|s| s != 0),
    raw: Some(data.clone()),
    ..User::default()
  };
  let tags: Vec<String> = base
    .list("ylzTags")
    .iter()
    .filter_map(|t| t.str("text"))
    .collect();
  if !tags.is_empty() {
    user.extra.insert("tags".into(), json!(tags));
  }
  insert(
    &mut user.extra,
    "praise_ratio",
    module.at("shop.praiseRatio").as_f64().map(Value::from),
  );
  user
}

/// The logged-in account (`mtop.idle.web.user.page.nav`).
pub fn me(data: &Value, id: &str, nick: Option<String>) -> User {
  let base = data.at("module.base");
  let mut stats = UserStats {
    followers: base.count("followers"),
    following: base.count("following"),
    ..UserStats::default()
  };
  for (key, path) in [
    ("sold", "soldCount"),
    ("bought", "purchaseCount"),
    ("favorites", "collectionCount"),
  ] {
    if let Some(n) = base.count(path) {
      stats.other.insert(key.into(), n);
    }
  }
  User {
    id: id.to_owned(),
    name: base.str("displayName").or(nick).unwrap_or_default(),
    url: Some(user_url(id)),
    avatar: base.str("avatar").map(https),
    stats,
    raw: Some(data.clone()),
    ..User::default()
  }
}

/// A decoded push in the shape of a history entry, or `None` for pushes
/// that are not messages (typing, reads, state changes). Fields are
/// numbered: `1.2` chat, `1.3` id, `1.5` time, `1.6.3.5` content,
/// `1.10` sender and summary.
pub fn push(decoded: &Value) -> Option<Value> {
  let m = decoded.at("1");
  let ext = m.at("10");
  ext.get("senderUserId")?;
  Some(json!({"message": {
    "messageId": m.str("3")?,
    "cid": m.str("2")?,
    "createAt": m.i64("5"),
    "content": {"contentType": 101, "custom": {"data": m.at("6.3.5"), "summary": m.at("6.3.2")}},
    "extension": ext,
  }}))
}
