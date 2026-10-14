//! Rankings (榜单): the rankings of a city (named by its sight list) and
//! the entries of one ranking, from its page
//! (`m.ctrip.com/webapp/xranking/cranking/crankingCity/<id>.html`, which
//! shows the first entries of a ranking). `hot` is a ranking of the city
//! in `CTRIP_CITY`.

use atp_core::{Collection, Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::{Api, Site};
use crate::refs;
use crate::sights::{self, Filter};
use crate::{cities, parse};

/// `hot --category`, with words of the ranking titles they pick.
const CATEGORIES: &[(&str, &[&str])] = &[
  ("must-see", &["必打卡"]),
  ("night", &["夜游", "夜"]),
  ("family", &["亲子"]),
  ("experience", &["体验"]),
];
pub const CATEGORY_NAMES: &[&str] = &["must-see", "night", "family", "experience"];

/// `rankings CITY`: the city's rankings.
pub async fn rankings(api: &Api, district: &str) -> Result<Page<Collection>> {
  let filter = Filter::default();
  let req = PageReq {
    cursor: None,
    size: 1,
  };
  let v = sights::attractions(api, district, &filter, &req).await?;
  Ok(Page::last(
    v.list("rankingList")
      .iter()
      .filter_map(collection)
      .collect(),
  ))
}

fn collection(v: &Value) -> Option<Collection> {
  let id = v.str("rankingId")?;
  Some(Collection {
    url: Some(refs::ranking_url(&id)),
    kind: "ranking".into(),
    name: v.str("title").unwrap_or_default(),
    items: v.u64("count").filter(|n| *n > 0),
    raw: Some(v.clone()),
    id,
    ..Collection::default()
  })
}

/// `ranking ID`: the entries the ranking's page shows.
pub async fn ranking(api: &Api, id: &str) -> Result<Page<Post>> {
  let html = api.page(&refs::ranking_url(id), Site::Mobile).await?;
  let state = parse::next_data(&html)
    .map(|d| d.at("props.pageProps.initialState").clone())
    .ok_or_else(|| Error::not_found(format!("Ctrip shows no ranking {id}")))?;
  let title = state.str("rankingDetail.title");
  let items: Vec<Post> = state
    .list("rankingItemInfo.itemList")
    .iter()
    .filter_map(parse::ranking_item)
    .map(|mut post| {
      if let Some(title) = &title {
        post.extra.insert("ranking".into(), title.clone().into());
      }
      post
    })
    .collect();
  if items.is_empty() {
    return Err(Error::not_found(format!("ranking {id} lists nothing")));
  }
  Ok(Page::last(items))
}

/// `hot`: a ranking of the city in `CTRIP_CITY` (Shanghai without it).
pub async fn hot(api: &Api, category: Option<&str>) -> Result<Page<Post>> {
  let district = cities::district(api, None).await?;
  let list = rankings(api, &district).await?.items;
  let words = CATEGORIES
    .iter()
    .find(|(name, _)| Some(*name) == category)
    .map_or(&["必打卡"][..], |(_, words)| *words);
  let chosen = list
    .iter()
    .find(|c| words.iter().any(|w| c.name.contains(w)))
    .or_else(|| list.first().filter(|_| category.is_none()))
    .ok_or_else(|| {
      let names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
      Error::not_found(format!(
        "no such ranking in city {district}; it has: {}",
        names.join(", ")
      ))
    })?;
  ranking(api, &chosen.id).await
}
