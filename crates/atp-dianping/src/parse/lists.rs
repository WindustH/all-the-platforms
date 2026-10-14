//! Search results (`#shop-all-list`, 15 shops a page), the discovery feed
//! (`__dx_dump__`, 80 notes a page) and a member's page.

use std::sync::LazyLock;

use atp_core::text::{html_to_text, parse_count};
use atp_core::{Media, Post, Price, User, Value, ValueExt};
use atp_mtsec::shops;
use regex::Regex;

use super::capture;
use crate::api::absolute;
use crate::refs::{member_url, note_url};

macro_rules! re {
  ($name:ident, $pattern:literal) => {
    static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).expect("valid"));
  };
}

re!(SHOP_ID, r#"data-shopid="([A-Za-z0-9]+)""#);
re!(NAME, r"<h4>([^<]+)</h4>");
re!(IMAGE, r#"<img[^>]*?(?:data-src|src)="([^"]+)""#);
re!(REVIEWS, r#"class="review-num"[^>]*>\s*<b>(\d+)</b>"#);
re!(
  MEAN_PRICE,
  r#"class="mean-price"[\s\S]*?<b>\s*[￥¥]?\s*(\d+(?:\.\d+)?)"#
);
re!(STARS, r"(?:star_|sml-str)(\d{2})\b");
re!(TAG, r#"<span class="tag">([^<]+)</span>"#);
re!(ADDRESS, r#"<span class="addr">([^<]+)</span>"#);
re!(SUB_SCORE, r"<span>([^<>]{1,6})<b>([\d.]+)</b></span>");
re!(MEMBER_NAME, r#"<h2 class="name">([^<]+)</h2>"#);
re!(
  MEMBER_AVATAR,
  r#"class="avatar"[\s\S]{0,200}?<img[^>]+src="([^"]+)""#
);
re!(MEMBER_CITY, r#"class="user-groun"[^>]*>([\s\S]*?)</span>"#);
re!(MEMBER_COUNTS, r"<strong>(\d+)</strong>");

/// The shops of a search page, and whether a next page is linked.
pub fn search(html: &str) -> (Vec<Post>, bool) {
  let Some(start) = html.find("shop-all-list") else {
    return (Vec::new(), false);
  };
  let list = &html[start..];
  let list = &list[..list.find("</ul>").unwrap_or(list.len())];
  let shops = list.split("<li").skip(1).filter_map(search_item).collect();
  (shops, html.contains("class=\"next\""))
}

fn search_item(item: &str) -> Option<Post> {
  let uuid = capture(&SHOP_ID, item)?;
  let mut post = Post {
    kind: "shop".into(),
    title: capture(&NAME, item),
    text: capture(&ADDRESS, item),
    url: Some(shops::www_url(&uuid)),
    media: capture(&IMAGE, item)
      .map(|u| Media::image(absolute(&u)))
      .into_iter()
      .collect(),
    tags: TAG
      .captures_iter(item)
      .map(|m| m[1].trim().to_owned())
      .collect(),
    price: capture(&MEAN_PRICE, item)
      .and_then(|p| p.parse().ok())
      .map(|p| Price {
        label: Some("人均".into()),
        ..Price::cny(p)
      }),
    id: uuid,
    ..Post::default()
  };
  post.metrics.comments = capture(&REVIEWS, item).and_then(|n| n.parse().ok());
  if let Some(stars) = capture(&STARS, item).and_then(|s| s.parse::<f64>().ok()) {
    post.extra.insert("stars".into(), (stars / 10.0).into());
  }
  let scores: Vec<Value> = SUB_SCORE
    .captures_iter(item)
    .map(|m| format!("{}:{}", &m[1], &m[2]).into())
    .collect();
  if !scores.is_empty() {
    post.extra.insert("scores".into(), scores.into());
  }
  Some(post)
}

/// The notes of a discovery page, and whether more pages follow.
pub fn feed(dump: &Value) -> (Vec<Post>, bool) {
  let nodes = dump.at("dump");
  let Some(list) = nodes
    .as_object()
    .and_then(|m| m.values().find(|n| n.get("feedList").is_some()))
  else {
    return (Vec::new(), false);
  };
  let items: Vec<Post> = list
    .list("feedList")
    .iter()
    .filter_map(|id| {
      let key = id.as_str().map_or_else(|| id.to_string(), str::to_owned);
      feed_item(nodes.at(&key))
    })
    .collect();
  let page = list.u64("pageNo").unwrap_or(1);
  let total = list.u64("total").unwrap_or_default();
  let more = !items.is_empty() && page * 80 < total;
  (items, more)
}

fn feed_item(v: &Value) -> Option<Post> {
  let id = v.str("contentId")?;
  let mut post = Post {
    kind: "note".into(),
    title: v.first_str(&["titleInfo.title", "headPicAlt"]),
    url: Some(note_url(&id)),
    author: v.str("authorInfo.name").map(|name| User {
      id: name.clone(),
      name,
      avatar: v.str("authorInfo.avatar").map(|a| absolute(&a)),
      ..User::default()
    }),
    media: v
      .str("headPic")
      .map(|u| Media::image(absolute(&u)))
      .into_iter()
      .collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  post.metrics.likes = v
    .str("authorInfo.likeCountDesc")
    .and_then(|n| parse_count(&n));
  if v.bool("headInfo.isVideo") == Some(true) {
    post.extra.insert("video".into(), true.into());
  }
  Some(post)
}

/// A member's page: name, avatar, city and the counts it shows (关注,
/// 粉丝, 互动).
pub fn member(html: &str, id: &str) -> Option<User> {
  let name = capture(&MEMBER_NAME, html)?;
  let counts: Vec<u64> = html
    .find("user_atten")
    .map(|at| &html[at..html.len().min(at + 1500)])
    .map(|block| {
      MEMBER_COUNTS
        .captures_iter(block)
        .filter_map(|m| m[1].parse().ok())
        .collect()
    })
    .unwrap_or_default();
  let mut user = User {
    id: id.to_owned(),
    name,
    url: Some(member_url(id)),
    avatar: capture(&MEMBER_AVATAR, html).map(|a| absolute(&a)),
    location: capture(&MEMBER_CITY, html).map(|c| html_to_text(&c).trim().to_owned()),
    ..User::default()
  };
  user.stats.following = counts.first().copied();
  user.stats.followers = counts.get(1).copied();
  if let Some(n) = counts.get(2) {
    user.stats.other.insert("interactions".into(), *n);
  }
  Some(user)
}
