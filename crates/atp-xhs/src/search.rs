//! Search: notes (in the web app's search sessions), users and topics
//! (through the creator center), suggestions and trending queries.

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::{
  Collection, Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt, json,
};
use serde::{Deserialize, Serialize};

use crate::api::Api;
use crate::refs::SOURCE_SEARCH;
use crate::{notes, parse, sign};

const SEARCH_NOTES: &str = "/api/sns/web/v1/search/notes";

const SUGGEST: &str = "/api/sns/web/v1/search/recommend";

const TRENDING: &str = "/api/sns/web/v1/search/trending/query";

/// `search --sort` values.
pub const SEARCH_SORTS: &[&str] = &["general", "popular", "latest", "comments", "collects"];

/// `search --filter` values: note type or publication time.
pub const SEARCH_FILTERS: &[&str] = &["video", "image", "day", "week", "half-year"];

// ── notes ───────────────────────────────────────────────────────────────

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let sort = sort_tag(query.sort.as_deref())?;
  let (note_type, time) = filter_tags(query.filter.as_deref())?;
  let type_tag = ["不限", "视频笔记", "普通笔记"][usize::from(note_type)];
  let keyword = query.keyword.trim();
  let page_no = page.number_or(1);
  let (search_id, fresh) = search_session(api, keyword, &format!("{sort}/{time}"), note_type);
  if fresh {
    prewarm(api, keyword, &search_id).await;
  }
  let body = json!({
    "keyword": keyword,
    "page": page_no,
    // The web client always asks for 20; other sizes get an empty answer.
    "page_size": 20,
    "search_id": search_id,
    "sort": sort,
    "note_type": note_type,
    "ext_flags": [],
    "filters": [
      {"tags": [sort], "type": "sort_type"},
      {"tags": [type_tag], "type": "filter_note_type"},
      {"tags": [time], "type": "filter_note_time"},
      {"tags": ["不限"], "type": "filter_note_range"},
      {"tags": ["不限"], "type": "filter_pos_distance"},
    ],
    "geo": "",
    "image_formats": ["jpg", "webp", "avif"],
  });
  let data = api.post(SEARCH_NOTES, &body).await?;
  if fresh {
    let _ = api
      .get("/api/sns/web/v1/search/recommend", &[("keyword", keyword)])
      .await;
  }
  let items = notes::listed(api, &data, SOURCE_SEARCH);
  let next = (data.bool("has_more") == Some(true)).then(|| (page_no + 1).to_string());
  Ok(Page::new(items, next))
}

/// The upstream tag of a `--sort` value.
fn sort_tag(sort: Option<&str>) -> Result<&'static str> {
  Ok(match sort.unwrap_or("general") {
    "general" => "general",
    "popular" => "popularity_descending",
    "latest" => "time_descending",
    "comments" => "comment_descending",
    "collects" => "collect_descending",
    other => return Err(Error::input(format!("unknown sort `{other}`"))),
  })
}

/// Note type and time tag of a `--filter` value: one filter at a time, a
/// note type or how recent the notes are.
fn filter_tags(filter: Option<&str>) -> Result<(u8, &'static str)> {
  Ok(match filter {
    None | Some("all") => (0, "不限"),
    Some("video") => (1, "不限"),
    Some("image") => (2, "不限"),
    Some("day") => (0, "一天内"),
    Some("week") => (0, "一周内"),
    Some("half-year") => (0, "半年内"),
    Some(other) => return Err(Error::input(format!("unknown filter `{other}`"))),
  })
}

/// What the web client sends before the first page of a new search. Errors are ignored.
async fn prewarm(api: &Api, keyword: &str, search_id: &str) {
  let request_id = sign::random::search_request_id(sign::now_ms());
  let onebox = json!({
    "keyword": keyword,
    "search_id": search_id,
    "biz_type": "web_search_user",
    "request_id": request_id,
  });
  if let Err(e) = api.post("/api/sns/web/v1/search/onebox", &onebox).await {
    tracing::debug!("search prewarm failed: {e}");
  }
  let params = [("keyword", keyword), ("search_id", search_id)];
  if let Err(e) = api.get("/api/sns/web/v1/search/filter", &params).await {
    tracing::debug!("search prewarm failed: {e}");
  }
}

#[derive(Serialize, Deserialize)]
struct SearchSession {
  id: String,
  used: u64,
}

const SEARCH_SESSION_TTL: u64 = 600;

/// One `search_id` per (keyword, sort, type) for ten minutes, so later pages
/// continue the same search; `true` when it was just created.
fn search_session(api: &Api, keyword: &str, sort: &str, note_type: u8) -> (String, bool) {
  let now = sign::now_ms() / 1000;
  let key = format!("{keyword}\u{1}{sort}\u{1}{note_type}");
  let store = &api.ctx.store;
  let mut map: BTreeMap<String, SearchSession> = store
    .cache_get("search-sessions", Duration::from_secs(SEARCH_SESSION_TTL))
    .unwrap_or_default();
  map.retain(|_, s| now.saturating_sub(s.used) <= SEARCH_SESSION_TTL);
  let fresh = !map.contains_key(&key);
  let session = map.entry(key).or_insert_with(|| SearchSession {
    id: sign::random::search_id(sign::now_ms()),
    used: now,
  });
  session.used = now;
  let id = session.id.clone();
  store.cache_put("search-sessions", &map);
  (id, fresh)
}

/// Completions the search box offers for `keyword`.
pub async fn suggest(api: &Api, keyword: &str) -> Result<Page<Collection>> {
  api.require_login()?;
  let data = api.get(SUGGEST, &[("keyword", keyword.trim())]).await?;
  let items = data
    .list("sug_items")
    .iter()
    .filter_map(|s| query(s.str("text")?, s.str("type")))
    .collect();
  Ok(Page::last(items))
}

/// What the search box suggests before typing (猜你想搜 / 热搜), personalized.
pub async fn trending(api: &Api) -> Result<Page<Collection>> {
  api.require_login()?;
  let params = [
    ("source", "Explore"),
    ("search_type", "trend"),
    ("last_query", ""),
    ("last_query_time", "0"),
    ("word_request_situation", "FIRST_ENTER"),
    ("hint_word", ""),
    ("hint_word_type", ""),
    ("hint_word_request_id", ""),
  ];
  let data = api.get(TRENDING, &params).await?;
  let items = data
    .list("queries")
    .iter()
    .filter_map(|q| {
      let word = q.first_str(&["search_word", "title"])?;
      // `type` is `<recall>#<strategy>#…`; the strategy says why it is offered.
      let why = q
        .str("type")
        .map(|t| t.split('#').nth(1).unwrap_or(&t).to_owned());
      query(word, why)
    })
    .collect();
  Ok(Page::last(items))
}

fn query(word: String, why: Option<String>) -> Option<Collection> {
  Some(Collection {
    id: word.clone(),
    kind: "query".into(),
    name: word,
    description: why,
    ..Collection::default()
  })
}

// ── users and topics (creator center) ───────────────────────────────────

pub async fn search_users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let (no, size) = (page.number_or(1), page.size_within(20));
  let body = json!({
    "keyword": query.keyword,
    "search_id": crate::sign::now_ms().to_string(),
    "page": {"page_size": size, "page": no},
  });
  let data = api
    .creator_post("/web_api/sns/v1/search/user_info", &body)
    .await?;
  let rows = match &data {
    Value::Array(a) => a.as_slice(),
    _ => ["user_info_dtos", "users", "items"]
      .iter()
      .map(|k| data.list(k))
      .find(|l| !l.is_empty())
      .unwrap_or_default(),
  };
  let items: Vec<User> = rows.iter().filter_map(parse::search_user).collect();
  Ok(numbered(items, no, size))
}

pub async fn search_topics(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
  api.require_login()?;
  let (no, size) = (page.number_or(1), page.size_within(20));
  let data = topic_search(api, &query.keyword, no, size).await?;
  let items: Vec<Collection> = topic_rows(&data).iter().filter_map(parse::topic).collect();
  Ok(numbered(items, no, size))
}

/// The best match of a topic name, as publishing tags a note with it.
pub async fn first_topic(api: &Api, name: &str) -> Result<Option<Value>> {
  let data = topic_search(api, name, 1, 20).await?;
  Ok(topic_rows(&data).first().cloned())
}

async fn topic_search(api: &Api, keyword: &str, page: u64, size: usize) -> Result<Value> {
  let body = json!({
    "keyword": keyword,
    "suggest_topic_request": {"title": "", "desc": ""},
    "page": {"page_size": size, "page": page},
  });
  api
    .creator_post("/web_api/sns/v1/search/topic", &body)
    .await
}

fn topic_rows(data: &Value) -> &[Value] {
  match data {
    Value::Array(a) => a,
    _ => data.list("topic_info_dtos"),
  }
}

/// Page-numbered listing without a `has_more` flag: a full page may have more.
fn numbered<T>(items: Vec<T>, page: u64, size: usize) -> Page<T> {
  let next = (items.len() >= size).then(|| (page + 1).to_string());
  Page::new(items, next)
}
