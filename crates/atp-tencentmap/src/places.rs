//! Place search (`/ws/place/v1/*`): search in a city, around a point,
//! nearby recommendations (explore), details and suggestions. Pages hold
//! at most 20 places; Tencent serves 200 per query.

use atp_core::geo::Coord;
use atp_core::{Error, Page, PageReq, Post, Query, Result, Value, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

const MAX_RESULTS: u64 = 200;

/// `--filter` names and the Tencent categories they stand for.
pub const CATEGORIES: &[(&str, &str)] = &[
  ("food", "美食"),
  ("hotel", "酒店宾馆"),
  ("shopping", "购物"),
  ("life", "生活服务"),
  ("scenic", "旅游景点"),
  ("entertainment", "娱乐休闲"),
  ("sports", "运动健身"),
  ("education", "教育学校"),
  ("medical", "医疗保健"),
  ("car", "汽车"),
  ("transport", "基础设施"),
  ("finance", "银行金融"),
  ("residential", "房产小区"),
  ("company", "公司企业"),
  ("government", "机构团体"),
  ("gas", "加油站"),
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

/// `category=NAME` of a `--filter` name, or of the value itself (a
/// Tencent category name or code).
fn category(filter: &str) -> String {
  let name = CATEGORIES
    .iter()
    .find(|(n, _)| *n == filter)
    .map_or(filter, |(_, c)| c);
  format!("category={name}")
}

/// Search in a city (`city:NAME`; nationwide otherwise) with an optional
/// category (`type:NAME`, `--filter`).
pub async fn search(api: &Api, query: &Query, req: &PageReq) -> Result<Page<Post>> {
  let mut words = Vec::new();
  let mut city = String::from("全国");
  let mut filter = query.filter.as_deref().map(category);
  for word in query.keyword.split_whitespace() {
    if let Some(c) = word.strip_prefix("city:") {
      city = c.to_owned();
    } else if let Some(t) = word.strip_prefix("type:") {
      filter = Some(category(t));
    } else {
      words.push(word);
    }
  }
  if words.is_empty() {
    return Err(Error::input("give a keyword"));
  }
  let params = vec![
    ("keyword", words.join(" ")),
    ("boundary", format!("region({city},0)")),
    ("filter", filter.unwrap_or_default()),
  ];
  list(api, "/ws/place/v1/search", params, req).await
}

/// Places around a point, nearest first: matching a keyword (radius at
/// most 1000 m), or Tencent's recommendations (200–1000 m) without one.
pub async fn nearby(
  api: &Api,
  location: &str,
  keyword: Option<&str>,
  filter: Option<&str>,
  radius: u32,
  req: &PageReq,
) -> Result<Page<Post>> {
  let center: Coord = refs::expect_point(location, api.system())?;
  let (path, radius) = match keyword {
    Some(_) => ("/ws/place/v1/search", radius.clamp(10, 1000)),
    None => ("/ws/place/v1/explore", radius.clamp(200, 1000)),
  };
  let params = vec![
    ("keyword", keyword.unwrap_or_default().to_owned()),
    (
      "boundary",
      format!("nearby({},{radius},0)", center.lat_lng()),
    ),
    ("filter", filter.map(category).unwrap_or_default()),
    ("orderby", "_distance".to_owned()),
  ];
  list(api, path, params, req).await
}

async fn list(
  api: &Api,
  path: &str,
  mut params: Vec<(&str, String)>,
  req: &PageReq,
) -> Result<Page<Post>> {
  let size = req.size_within(20) as u64;
  let number = req.number_or(1);
  params.push(("page_size", size.to_string()));
  params.push(("page_index", number.to_string()));
  let v = api.get(path, &params).await?;
  if v.list("data").is_empty() && !v.list("cluster").is_empty() {
    return Err(cities(&v));
  }
  let items: Vec<Post> = v.list("data").iter().filter_map(parse::place).collect();
  let total = v.u64("count").unwrap_or(MAX_RESULTS).min(MAX_RESULTS);
  let more = items.len() as u64 >= size && number * size < total;
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

/// A nationwide search matched many cities: say which.
fn cities(v: &Value) -> Error {
  let names: Vec<String> = v
    .list("cluster")
    .iter()
    .take(8)
    .filter_map(|c| {
      Some(format!(
        "{} ({})",
        c.str("title")?,
        c.u64("count").unwrap_or(0)
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
  let id = refs::place_id(arg)?;
  let v = api
    .get("/ws/place/v1/detail", &[("id", id.clone())])
    .await?;
  v.list("data")
    .first()
    .and_then(parse::place)
    .ok_or_else(|| Error::not_found(format!("no Tencent place {id}")))
}

/// Suggestions while typing.
pub async fn tips(
  api: &Api,
  keyword: &str,
  city: Option<&str>,
  location: Option<&str>,
) -> Result<Page<Post>> {
  let location = location
    .map(|l| refs::expect_point(l, api.system()))
    .transpose()?;
  let params = [
    ("keyword", keyword.to_owned()),
    ("region", city.unwrap_or_default().to_owned()),
    (
      "region_fix",
      city.map(|_| "1".to_owned()).unwrap_or_default(),
    ),
    (
      "location",
      location.map(|c| c.lat_lng()).unwrap_or_default(),
    ),
    ("page_index", "1".to_owned()),
    ("page_size", "20".to_owned()),
  ];
  let v = api.get("/ws/place/v1/suggestion", &params).await?;
  Ok(Page::last(
    v.list("data")
      .iter()
      .filter_map(parse::suggestion)
      .collect(),
  ))
}
