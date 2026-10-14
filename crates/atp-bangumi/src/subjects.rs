//! Subjects (anime, books, music, games, real-life shows) and what hangs
//! off them: search, the airing calendar, browsing by type, one subject,
//! character, person or episode, the cast, staff, related subjects and
//! episodes.

use atp_core::paging::slice;
use atp_core::{Error, Page, PageReq, Post, Query, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse::{self, TYPES};
use crate::refs::{self, Item};

pub const SORTS: &[&str] = &["match", "heat", "rank", "score"];
pub const FILTERS: &[&str] = &["anime", "book", "music", "game", "real"];
/// `hot --category`: the calendar of one day, or the whole week.
pub const DAYS: &[&str] = &[
  "today", "mon", "tue", "wed", "thu", "fri", "sat", "sun", "week",
];
/// The most rows a v0 listing returns at once.
const PAGE_MAX: usize = 50;

/// `(offset, limit)` of a v0 listing.
fn window(page: &PageReq, max: usize) -> (u64, usize) {
  (page.number_or(0), page.size_within(max))
}

/// One page of a v0 listing (`{data, total, offset}`), each row through `item`.
fn paged(v: &Value, offset: u64, item: impl Fn(&Value) -> Post) -> Page<Post> {
  let rows = v.list("data");
  let next = offset + rows.len() as u64;
  let more = !rows.is_empty() && v.u64("total").is_some_and(|total| next < total);
  Page::new(
    rows.iter().map(item).collect(),
    more.then(|| next.to_string()),
  )
}

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let (offset, limit) = window(page, PAGE_MAX);
  let mut filter = json!({});
  if let Some(name) = &query.filter {
    let code = parse::code(TYPES, name)
      .ok_or_else(|| Error::input(format!("unknown subject type: {name}")))?;
    filter["type"] = json!([code]);
  }
  let body = json!({
    "keyword": query.keyword,
    "sort": query.sort.as_deref().unwrap_or("match"),
    "filter": filter,
  });
  let params = vec![("limit", limit.to_string()), ("offset", offset.to_string())];
  let v = api.post("/v0/search/subjects", params, body).await?;
  Ok(paged(&v, offset, parse::subject))
}

/// A subject, character, person or episode.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  Ok(match refs::item(arg)? {
    Item::Subject(id) => parse::subject(&api.get(&format!("/v0/subjects/{id}"), Vec::new()).await?),
    Item::Character(id) => {
      let v = api.get(&format!("/v0/characters/{id}"), Vec::new()).await?;
      parse::figure(&v, "character")
    }
    Item::Person(id) => {
      let v = api.get(&format!("/v0/persons/{id}"), Vec::new()).await?;
      parse::figure(&v, "person")
    }
    Item::Episode(id) => parse::episode(&api.get(&format!("/v0/episodes/{id}"), Vec::new()).await?),
  })
}

/// The airing calendar (每日放送): one weekday, today by default, or the
/// whole week from Monday.
pub async fn calendar(api: &Api, day: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let wanted = match day.unwrap_or("today") {
    "week" => None,
    "today" => Some(jiff::Zoned::now().weekday().to_monday_one_offset() as u64),
    name => DAYS
      .iter()
      .position(|d| *d == name)
      .map(|i| i as u64)
      .filter(|i| (1..=7).contains(i)),
  };
  let v = api.get("/calendar", Vec::new()).await?;
  let mut items = Vec::new();
  for day in v.as_array().map(Vec::as_slice).unwrap_or_default() {
    let number = day.u64("weekday.id");
    if wanted.is_some() && number != wanted {
      continue;
    }
    for row in day.list("items") {
      let mut post = parse::subject(row);
      if let Some(name) = day.str("weekday.en") {
        post.extra.insert("weekday".into(), json!(name));
      }
      items.push(post);
    }
  }
  Ok(slice(items, page))
}

/// Subjects of one type, by rank or date, optionally of one year / month.
pub async fn browse(
  api: &Api,
  kind: &str,
  sort: &str,
  year: Option<u32>,
  month: Option<u32>,
  page: &PageReq,
) -> Result<Page<Post>> {
  let code = parse::code(TYPES, kind)
    .ok_or_else(|| Error::input(format!("unknown subject type: {kind}")))?;
  let (offset, limit) = window(page, PAGE_MAX);
  let mut params = vec![
    ("type", code.to_string()),
    ("sort", sort.to_owned()),
    ("limit", limit.to_string()),
    ("offset", offset.to_string()),
  ];
  params.extend(year.map(|y| ("year", y.to_string())));
  params.extend(month.map(|m| ("month", m.to_string())));
  let v = api.get("/v0/subjects", params).await?;
  Ok(paged(&v, offset, parse::subject))
}

/// The characters (with their voice actors), staff or related subjects of
/// a subject; Bangumi returns each list whole.
pub async fn linked(api: &Api, arg: &str, what: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = refs::subject(arg)?;
  let v = api
    .get(&format!("/v0/subjects/{id}/{what}"), Vec::new())
    .await?;
  let rows = v.as_array().map(Vec::as_slice).unwrap_or_default();
  let items = rows
    .iter()
    .map(|row| match what {
      "characters" => parse::figure(row, "character"),
      "persons" => parse::figure(row, "person"),
      _ => parse::relation(row),
    })
    .collect();
  Ok(slice(items, page))
}

pub async fn episodes(
  api: &Api,
  arg: &str,
  kind: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  let id = refs::subject(arg)?;
  let (offset, limit) = window(page, 200);
  let mut params = vec![
    ("subject_id", id.to_string()),
    ("limit", limit.to_string()),
    ("offset", offset.to_string()),
  ];
  if let Some(kind) = kind {
    let code = parse::EPISODE_TYPES
      .iter()
      .position(|t| *t == kind)
      .ok_or_else(|| Error::input(format!("unknown episode type: {kind}")))?;
    params.push(("type", code.to_string()));
  }
  let v = api.get("/v0/episodes", params).await?;
  Ok(paged(&v, offset, parse::episode))
}
