//! Films, shows, books and albums: search, one subject, reviews, the
//! ranked lists (榜单) and the film chart.

use atp_core::paging::slice;
use atp_core::{Error, ErrorCode, Page, PageReq, Post, Query, Result, Value, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Subject, Target};

/// `search --filter`, with the mobile search's type codes.
const FILTERS: &[(&str, &str)] = &[
  ("movie", "1002"),
  ("book", "1001"),
  ("music", "1003"),
  ("group", "1019"),
];
pub const FILTER_NAMES: &[&str] = &["movie", "book", "music", "group"];
/// Results per page of the mobile search.
const SEARCH_PAGE: u64 = 10;

/// `hot --category`: Douban's lists (`subject_collection`) by a short name,
/// and `new`, the film chart page.
pub const CHARTS: &[(&str, &str)] = &[
  ("hot", "subject_real_time_hotest"),
  ("movie", "movie_real_time_hotest"),
  ("top250", "movie_top250"),
  ("weekly", "movie_weekly_best"),
  ("new", ""),
  ("showing", "movie_showing"),
  ("latest", "movie_latest"),
  ("tv", "tv_real_time_hotest"),
  ("tv-chinese", "tv_chinese_best_weekly"),
  ("tv-global", "tv_global_best_weekly"),
  ("book-top250", "book_top250"),
  ("fiction", "book_fiction_hot_weekly"),
  ("nonfiction", "book_nonfiction_hot_weekly"),
  ("music", "music_single"),
];
pub const CHART_NAMES: &[&str] = &[
  "hot",
  "movie",
  "top250",
  "weekly",
  "new",
  "showing",
  "latest",
  "tv",
  "tv-chinese",
  "tv-global",
  "book-top250",
  "fiction",
  "nonfiction",
  "music",
];
/// The film chart page (新片榜 and the weekly box lists).
const CHART_PAGE: &str = "https://movie.douban.com/chart";
/// The most entries one list request returns.
const LIST_MAX: usize = 50;

/// The mobile page of a subject, the `Referer` of calls about it.
pub fn page_of(subject: &Subject) -> String {
  format!("/{}/subject/{}/", subject.kind, subject.id)
}

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let name = query.filter.as_deref().unwrap_or("movie");
  let code = FILTERS
    .iter()
    .find(|(n, _)| *n == name)
    .map(|(_, code)| *code)
    .ok_or_else(|| Error::input(format!("unknown search filter: {name}")))?;
  let number = page.number_or(0);
  let params = [
    ("q", query.keyword.clone()),
    ("t", code.to_owned()),
    ("p", number.to_string()),
  ];
  let referer = format!(
    "/search/?query={}&type={code}",
    atp_core::text::encode_uri_component(&query.keyword)
  );
  let v = api.mobile("/j/search/", &referer, &params).await?;
  let items = parse::search(&v.str("html").unwrap_or_default());
  let shown = (number + 1) * v.u64("limit").unwrap_or(SEARCH_PAGE);
  let more = !items.is_empty() && v.u64("count").is_some_and(|count| shown < count);
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

/// A subject, review, group topic or group.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  match refs::target(arg)? {
    Target::Subject(subject) => Ok(parse::subject(&subject_json(api, &subject).await?)),
    Target::Review(id) => {
      let page = format!("/movie/review/{id}/");
      Ok(parse::review(
        &api.get(&format!("/review/{id}"), &page, &[]).await?,
      ))
    }
    Target::Topic(topic) => crate::groups::topic(api, &topic).await,
    Target::Group(id) => crate::groups::group(api, &id).await,
  }
}

/// A subject's payload. Films and shows share `movie.douban.com` links, so a
/// `movie` that is not found is asked for again as a `tv` show.
pub async fn subject_json(api: &Api, subject: &Subject) -> Result<Value> {
  let path = format!("/{}/{}", subject.kind, subject.id);
  match api.get(&path, &page_of(subject), &[]).await {
    Err(e) if e.code == ErrorCode::NotFound && subject.kind == "movie" => {
      let tv = Subject {
        kind: "tv".into(),
        id: subject.id.clone(),
      };
      api.get(&format!("/tv/{}", tv.id), &page_of(&tv), &[]).await
    }
    other => other,
  }
}

/// Reviews (影评 / 书评 / 乐评) of a subject.
pub async fn reviews(
  api: &Api,
  arg: &str,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  let subject = refs::subject_ref(arg)?;
  let start = page.number_or(0);
  let params = [
    ("start", start.to_string()),
    ("count", page.size_within(LIST_MAX).to_string()),
    ("order_by", sort.unwrap_or("hot").to_owned()),
  ];
  let path = format!("/{}/{}/reviews", subject.kind, subject.id);
  let referer = format!("{}reviews", page_of(&subject));
  let v = api.get(&path, &referer, &params).await?;
  let items: Vec<Post> = v.list("reviews").iter().map(parse::review).collect();
  let next = next_start(&v, start, items.len());
  Ok(Page::new(items, next))
}

/// The next `start` of a `{start, count, total}` listing, unless it ended.
pub fn next_start(v: &Value, start: u64, got: usize) -> Option<String> {
  let next = start + got as u64;
  let more = got > 0 && v.u64("total").is_some_and(|total| next < total);
  more.then(|| next.to_string())
}

/// `hot`: one of [`CHARTS`] (`hot`, Douban's real-time list, by default).
pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let name = category.unwrap_or("hot");
  let id = CHARTS
    .iter()
    .find(|(n, _)| *n == name)
    .map(|(_, id)| *id)
    .ok_or_else(|| Error::input(format!("unknown list: {name}")))?;
  if id.is_empty() {
    return new_films(api, page).await;
  }
  list(api, id, page).await
}

/// Any of Douban's lists by its id (the `subject_collection/<id>` of its
/// link), in order.
pub async fn list(api: &Api, id: &str, page: &PageReq) -> Result<Page<Post>> {
  let start = page.number_or(0);
  let params = [
    ("start", start.to_string()),
    ("count", page.size_within(LIST_MAX).to_string()),
  ];
  let path = format!("/subject_collection/{id}/items");
  let v = api
    .get(&path, &format!("/subject_collection/{id}"), &params)
    .await?;
  let rows = v.list("subject_collection_items");
  let items: Vec<Post> = rows
    .iter()
    .enumerate()
    .map(|(i, row)| parse::ranked(row, start as usize + i + 1))
    .collect();
  let next = next_start(&v, start, items.len());
  Ok(Page::new(items, next))
}

/// The film chart page's 新片榜 (new films of the last weeks, by rating).
async fn new_films(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let html = api.html(CHART_PAGE).await?;
  let films = parse::chart(&html);
  if films.is_empty() {
    return Err(Error::upstream(
      "found no films on movie.douban.com/chart (the page may have changed)",
    ));
  }
  Ok(slice(films, page))
}

/// The id of a list from its link (`…/subject_collection/<id>`) or the id.
pub fn list_id(arg: &str) -> Result<String> {
  let id = arg
    .trim()
    .trim_end_matches('/')
    .rsplit('/')
    .next()
    .unwrap_or_default()
    .split('?')
    .next()
    .unwrap_or_default();
  let ok = !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
  if ok {
    Ok(id.to_owned())
  } else {
    Err(Error::input(format!(
      "not a Douban list: {arg} (give a subject_collection link or its id)"
    )))
  }
}
