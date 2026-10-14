//! Places (POI 2.0, `/v5/place/*`): keyword search, nearby search, details,
//! and input tips. Pages hold at most 25 places; Amap serves 200 per query.

use atp_core::{Error, Page, PageReq, Post, Query, Result, Value, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

/// Field groups lists ask for; details ask for every group.
const LIST_FIELDS: &str = "business,photos";
const DETAIL_FIELDS: &str = "children,business,indoor,navi,photos";
const MAX_RESULTS: u64 = 200;

/// `search --filter` names and the Amap type codes they stand for.
pub const CATEGORIES: &[(&str, &str)] = &[
  ("food", "050000"),
  ("shopping", "060000"),
  ("life", "070000"),
  ("sports", "080000"),
  ("medical", "090000"),
  ("hotel", "100000"),
  ("scenic", "110000"),
  ("residential", "120000"),
  ("government", "130000"),
  ("education", "140000"),
  ("transport", "150000"),
  ("finance", "160000"),
  ("company", "170000"),
  ("car", "010000"),
  ("gas", "010100"),
  ("parking", "150900"),
  ("subway", "150500"),
  ("bus", "150700"),
  ("airport", "150100"),
  ("train", "150200"),
  ("bank", "160100"),
];

pub const FILTERS: &[&str] = &[
  "food",
  "shopping",
  "life",
  "sports",
  "medical",
  "hotel",
  "scenic",
  "residential",
  "government",
  "education",
  "transport",
  "finance",
  "company",
  "car",
  "gas",
  "parking",
  "subway",
  "bus",
  "airport",
  "train",
  "bank",
];

/// The type code of a `--filter` name, or the value itself (a raw code).
pub fn type_code(filter: &str) -> String {
  CATEGORIES
    .iter()
    .find(|(name, _)| *name == filter)
    .map_or_else(|| filter.to_owned(), |(_, code)| (*code).to_owned())
}

/// A search query: the keyword with `city:NAME` and `type:CODE` tokens taken out.
struct Terms {
  keywords: String,
  city: String,
  types: String,
}

fn terms(query: &Query) -> Terms {
  let mut t = Terms {
    keywords: String::new(),
    city: String::new(),
    types: query.filter.as_deref().map(type_code).unwrap_or_default(),
  };
  let mut words = Vec::new();
  for word in query.keyword.split_whitespace() {
    if let Some(city) = word.strip_prefix("city:") {
      t.city = city.to_owned();
    } else if let Some(code) = word.strip_prefix("type:") {
      t.types = code.to_owned();
    } else {
      words.push(word);
    }
  }
  t.keywords = words.join(" ");
  t
}

pub async fn search(api: &Api, query: &Query, req: &PageReq) -> Result<Page<Post>> {
  let t = terms(query);
  if t.keywords.is_empty() && t.types.is_empty() {
    return Err(Error::input("give a keyword (or type:CODE / --filter)"));
  }
  let limit = if t.city.is_empty() { "" } else { "true" };
  let params = vec![
    ("keywords", t.keywords),
    ("types", t.types),
    ("region", t.city),
    ("city_limit", limit.to_owned()),
  ];
  list(api, "/v5/place/text", params, req).await
}

/// Places around `location`, nearest first.
pub async fn nearby(
  api: &Api,
  location: &str,
  keyword: Option<&str>,
  filter: Option<&str>,
  radius: u32,
  req: &PageReq,
) -> Result<Page<Post>> {
  let params = vec![
    ("location", refs::expect_location(location, api.system())?),
    ("keywords", keyword.unwrap_or_default().to_owned()),
    ("types", filter.map(type_code).unwrap_or_default()),
    ("radius", radius.min(50_000).to_string()),
    ("sortrule", "distance".to_owned()),
  ];
  list(api, "/v5/place/around", params, req).await
}

async fn list(
  api: &Api,
  path: &str,
  mut params: Vec<(&str, String)>,
  req: &PageReq,
) -> Result<Page<Post>> {
  let size = req.size_within(25) as u64;
  let number = req.number_or(1);
  params.push(("page_size", size.to_string()));
  params.push(("page_num", number.to_string()));
  params.push(("show_fields", LIST_FIELDS.to_owned()));
  let v = api.get(path, &params).await?;
  let items: Vec<Post> = v.list("pois").iter().filter_map(parse::poi).collect();
  let more = items.len() as u64 >= size && number * size < MAX_RESULTS;
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = refs::poi_id(arg)?;
  let params = [
    ("id", id.clone()),
    ("show_fields", DETAIL_FIELDS.to_owned()),
  ];
  let v = api.get("/v5/place/detail", &params).await?;
  let mut post = v
    .list("pois")
    .first()
    .and_then(parse::poi)
    .ok_or_else(|| Error::not_found(format!("no Amap place {id}")))?;
  let children: Vec<Value> = v
    .at("pois.0.children")
    .as_array()
    .cloned()
    .unwrap_or_default();
  if !children.is_empty() {
    post.extra.insert("children".into(), Value::from(children));
  }
  Ok(post)
}

/// Suggestions while typing: places, bus lines and queries.
pub async fn tips(
  api: &Api,
  keyword: &str,
  city: Option<&str>,
  location: Option<&str>,
) -> Result<Page<Post>> {
  let system = api.system();
  let location = location
    .map(|l| refs::expect_location(l, system))
    .transpose()?;
  let params = [
    ("keywords", keyword.to_owned()),
    ("city", city.unwrap_or_default().to_owned()),
    (
      "citylimit",
      city.map(|_| "true".to_owned()).unwrap_or_default(),
    ),
    ("location", location.unwrap_or_default()),
  ];
  let v = api.get("/v3/assistant/inputtips", &params).await?;
  Ok(Page::last(
    v.list("tips").iter().filter_map(parse::tip).collect(),
  ))
}
