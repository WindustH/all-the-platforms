//! Pinduoduo payloads → core model. Goods are posts (`kind` `item`) with
//! their price in `price` (PDD shows the group-buy price; prices arrive in
//! fen, or as yuan strings in page stores); shops are users; reviews are
//! comments; hot search words are posts of kind `keyword`.

mod ddk;
mod goods;
mod page;

use atp_core::{Extra, Media, Price, User, Value, ValueExt, json};

use crate::api::{image_url, mall_url};

pub use ddk::{category, ddk_goods};
pub use goods::{goods, goods_list, hot_word, tab};
pub use page::{detail, mall, review};

/// The store of `window.rawData = {...}` in a page (`stores.store`, or
/// `store` in older pages).
pub fn raw_data(html: &str) -> Option<Value> {
  let start = html.find("window.rawData")?;
  let open = start + html[start..].find('{')?;
  let mut stream = serde_json::Deserializer::from_str(&html[open..]).into_iter::<Value>();
  let v = stream.next()?.ok()?;
  let store = v
    .get("stores")
    .and_then(|s| s.get("store"))
    .or_else(|| v.get("store"))
    .cloned()?;
  Some(store)
}

fn insert(extra: &mut Extra, key: &str, value: Option<Value>) {
  if let Some(value) = value.filter(|v| !v.is_null() && v != "" && v != &json!([])) {
    extra.insert(key.into(), value);
  }
}

/// An amount in fen (`14899`) as yuan.
fn fen(v: &Value, path: &str) -> Option<f64> {
  v.f64(path).filter(|n| *n > 0.0).map(|n| n / 100.0)
}

/// An amount in yuan, as a number or a string such as `¥12.9`.
fn yuan(v: &Value, path: &str) -> Option<f64> {
  let value = v.at(path);
  value
    .as_f64()
    .or_else(|| {
      value
        .as_str()?
        .trim()
        .trim_start_matches(['¥', '￥'])
        .trim_end_matches('起')
        .parse()
        .ok()
    })
    .filter(|n| *n > 0.0)
}

/// A price with its crossed-out amount (when higher) and wording.
fn price(amount: Option<f64>, original: Option<f64>, label: Option<String>) -> Option<Price> {
  let mut price = Price::cny(amount?);
  price.original = original.filter(|o| *o > price.amount + 0.001);
  price.label = label.filter(|l| !l.is_empty());
  Some(price)
}

/// A shop as the author of its goods.
fn shop(id: Option<String>, name: Option<String>, logo: Option<String>) -> Option<User> {
  let id = id.filter(|i| !i.is_empty() && i != "0");
  let name = name.filter(|n| !n.is_empty());
  if id.is_none() && name.is_none() {
    return None;
  }
  Some(User {
    url: id.as_deref().map(mall_url),
    name: name.unwrap_or_default(),
    id: id.unwrap_or_default(),
    avatar: logo.map(|l| image_url(&l)),
    ..User::default()
  })
}

/// Images from a list of links or of objects with a link, without repeats.
fn images<'a>(lists: impl IntoIterator<Item = &'a Value>, keys: &[&str]) -> Vec<Media> {
  let mut seen: Vec<String> = Vec::new();
  for v in lists {
    let url = v.as_str().map(str::to_owned).or_else(|| v.first_str(keys));
    if let Some(url) = url.filter(|u| !u.is_empty()).map(|u| image_url(&u))
      && !seen.contains(&url)
    {
      seen.push(url);
    }
  }
  seen.into_iter().map(Media::image).collect()
}

/// The first id among `paths` (numbers or strings, `0` meaning none).
fn id(v: &Value, paths: &[&str]) -> Option<String> {
  paths.iter().filter_map(|p| v.str(p)).find(|s| s != "0")
}

/// A count inside a tip such as `已拼10万+件` or `10万+`.
fn sold(tip: &str) -> Option<u64> {
  let start = tip.find(|c: char| c.is_ascii_digit())?;
  let number: String = tip[start..]
    .chars()
    .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '万' | '亿'))
    .collect();
  atp_core::text::parse_count(&number)
}
