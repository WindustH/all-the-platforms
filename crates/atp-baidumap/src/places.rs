//! Place search 2.0 (`/place/v2/*`): search in a region, around a point,
//! details and suggestions. Pages hold at most 20 places; Baidu serves
//! 150 per query.

use atp_core::{Error, Page, PageReq, Post, Query, Result, Value, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

const MAX_RESULTS: u64 = 150;

/// `--filter` names and the Baidu categories (`tag`) they stand for.
pub const CATEGORIES: &[(&str, &str)] = &[
  ("food", "美食"),
  ("hotel", "酒店"),
  ("shopping", "购物"),
  ("life", "生活服务"),
  ("beauty", "丽人"),
  ("scenic", "旅游景点"),
  ("entertainment", "休闲娱乐"),
  ("sports", "运动健身"),
  ("education", "教育培训"),
  ("medical", "医疗"),
  ("car", "汽车服务"),
  ("transport", "交通设施"),
  ("finance", "金融"),
  ("residential", "房地产"),
  ("company", "公司企业"),
  ("government", "政府机构"),
  ("gas", "加油加气站"),
  ("parking", "停车场"),
  ("subway", "地铁站"),
  ("bus", "公交车站"),
  ("airport", "飞机场"),
  ("train", "火车站"),
  ("bank", "银行"),
];

pub const FILTERS: &[&str] = &[
  "food",
  "hotel",
  "shopping",
  "life",
  "beauty",
  "scenic",
  "entertainment",
  "sports",
  "education",
  "medical",
  "car",
  "transport",
  "finance",
  "residential",
  "company",
  "government",
  "gas",
  "parking",
  "subway",
  "bus",
  "airport",
  "train",
  "bank",
];

/// The Baidu category of a `--filter` name, or the value itself.
pub fn tag(filter: &str) -> String {
  CATEGORIES
    .iter()
    .find(|(name, _)| *name == filter)
    .map_or_else(|| filter.to_owned(), |(_, t)| (*t).to_owned())
}

/// Search in a region: `city:NAME` keeps results inside a city (nationwide
/// otherwise), `type:TAG` or `--filter` sets the category.
pub async fn search(api: &Api, query: &Query, req: &PageReq) -> Result<Page<Post>> {
  let mut words = Vec::new();
  let mut city = String::new();
  let mut kind = query.filter.as_deref().map(tag).unwrap_or_default();
  for word in query.keyword.split_whitespace() {
    if let Some(c) = word.strip_prefix("city:") {
      city = c.to_owned();
    } else if let Some(t) = word.strip_prefix("type:") {
      kind = t.to_owned();
    } else {
      words.push(word);
    }
  }
  let keyword = if words.is_empty() {
    kind.clone()
  } else {
    words.join(" ")
  };
  if keyword.is_empty() {
    return Err(Error::input("give a keyword (or type:TAG / --filter)"));
  }
  let limit = if city.is_empty() { "" } else { "true" };
  let params = vec![
    ("query", keyword),
    ("tag", kind),
    (
      "region",
      if city.is_empty() {
        "全国".into()
      } else {
        city
      },
    ),
    ("city_limit", limit.to_owned()),
  ];
  list(api, params, req).await
}

/// Places within `radius` of a point.
pub async fn nearby(
  api: &Api,
  location: &str,
  keyword: Option<&str>,
  filter: Option<&str>,
  radius: u32,
  req: &PageReq,
) -> Result<Page<Post>> {
  let center = refs::expect_point(location, api.system(), api.wire())?;
  let kind = filter.map(tag).unwrap_or_default();
  let keyword = keyword.map_or_else(|| kind.clone(), str::to_owned);
  if keyword.is_empty() {
    return Err(Error::input("Baidu needs a keyword or --filter"));
  }
  let params = vec![
    ("query", keyword),
    ("tag", kind),
    ("location", center.lat_lng()),
    ("radius", radius.clamp(1, 50_000).to_string()),
    ("radius_limit", "true".to_owned()),
  ];
  list(api, params, req).await
}

async fn list(api: &Api, mut params: Vec<(&str, String)>, req: &PageReq) -> Result<Page<Post>> {
  let size = req.size_within(20) as u64;
  let number = req.number_or(0);
  let wire = api.wire();
  params.extend([
    ("scope", "2".to_owned()),
    ("coord_type", refs::number(wire).to_owned()),
    ("ret_coordtype", returned(wire)),
    ("page_size", size.to_string()),
    ("page_num", number.to_string()),
  ]);
  let v = api.get("/place/v2/search", &params).await?;
  if v.str("result_type").as_deref() == Some("city_type") {
    return Err(cities(&v));
  }
  let items: Vec<Post> = v
    .list("results")
    .iter()
    .filter_map(|p| parse::place(p, wire))
    .collect();
  let total = v.u64("total").unwrap_or(MAX_RESULTS).min(MAX_RESULTS);
  let more = items.len() as u64 >= size && (number + 1) * size < total;
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

/// `ret_coordtype` asking for `wire` (BD-09 is the default, so empty).
pub fn returned(wire: atp_core::geo::System) -> String {
  match wire {
    atp_core::geo::System::Bd09 => String::new(),
    other => refs::ll(other).to_owned(),
  }
}

/// A nationwide search matched many cities: say which.
fn cities(v: &Value) -> Error {
  let names: Vec<String> = v
    .list("results")
    .iter()
    .take(8)
    .filter_map(|c| {
      Some(format!(
        "{} ({})",
        c.str("name")?,
        c.u64("num").unwrap_or(0)
      ))
    })
    .collect();
  Error::input(format!(
    "the keyword matches places in many cities: {}",
    names.join(", ")
  ))
  .with_hint("add city:NAME to the search")
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let uid = refs::uid(arg)?;
  let wire = api.wire();
  let params = [
    ("uid", uid.clone()),
    ("scope", "2".to_owned()),
    ("ret_coordtype", returned(wire)),
  ];
  let v = api.get("/place/v2/detail", &params).await?;
  parse::place(v.at("result"), wire)
    .ok_or_else(|| Error::not_found(format!("no Baidu place {uid}")))
}

/// Suggestions while typing (at most 10).
pub async fn tips(
  api: &Api,
  keyword: &str,
  city: Option<&str>,
  location: Option<&str>,
) -> Result<Page<Post>> {
  let wire = api.wire();
  let location = location
    .map(|l| refs::expect_point(l, api.system(), wire))
    .transpose()?;
  let params = [
    ("query", keyword.to_owned()),
    ("region", city.unwrap_or("全国").to_owned()),
    (
      "city_limit",
      city.map(|_| "true".to_owned()).unwrap_or_default(),
    ),
    (
      "location",
      location.map(|c| c.lat_lng()).unwrap_or_default(),
    ),
    ("coord_type", refs::number(wire).to_owned()),
    ("ret_coordtype", returned(wire)),
  ];
  let v = api.get("/place/v2/suggestion", &params).await?;
  let items = v
    .list("result")
    .iter()
    .filter_map(|s| parse::suggestion(s, wire))
    .collect();
  Ok(Page::last(items))
}
