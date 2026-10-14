//! Search (the desktop player's, which answers without a login): songs,
//! albums, singers, playlists, songs by their lyrics, and users.

use atp_core::{Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse;

/// `search --filter` values; the first is the default.
pub const FILTERS: &[&str] = &["song", "album", "artist", "playlist", "lyrics"];
/// Results one page fetches at most.
const PAGE_SIZE: usize = 50;
/// What a search id is built from, as the web player builds it.
const SEARCH_ID_KIND: u64 = 18_014_398_509_481_984;
const SEARCH_ID_RANDOM: u64 = 4_294_967_296;

/// One kind of result: the upstream type, where the items are, the code
/// its search ids start with.
struct Kind {
  code: u64,
  items: &'static str,
  id_kind: u64,
}

fn kind(filter: &str) -> Kind {
  let (code, items, id_kind) = match filter {
    "artist" => (1, "singer.list", 1),
    "album" => (2, "album.list", 4),
    "playlist" => (3, "songlist.list", 6),
    "lyrics" => (7, "song.list", 5),
    "user" => (8, "user.list", 13),
    _ => (0, "song.list", 3),
  };
  Kind {
    code,
    items,
    id_kind,
  }
}

pub async fn posts(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let filter = query.filter.as_deref().unwrap_or(FILTERS[0]);
  let parse: fn(&Value) -> Option<Post> = match filter {
    "album" => parse::album,
    "artist" => parse::artist,
    "playlist" => parse::playlist,
    _ => parse::song,
  };
  let (items, next) = run(api, &query.keyword, kind(filter), page).await?;
  Ok(Page::new(items.iter().filter_map(parse).collect(), next))
}

pub async fn users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let (items, next) = run(api, &query.keyword, kind("user"), page).await?;
  Ok(Page::new(
    items.iter().filter_map(parse::user).collect(),
    next,
  ))
}

/// The items of one page and the next cursor (`page:size`, the size kept
/// from the first page so pages line up).
async fn run(
  api: &Api,
  keyword: &str,
  kind: Kind,
  page: &PageReq,
) -> Result<(Vec<Value>, Option<String>)> {
  let cursor = page.cursor.as_deref().unwrap_or_default();
  let (number, size) = match cursor.split_once(':') {
    Some((n, s)) => (n.parse().unwrap_or(1), s.parse().unwrap_or(PAGE_SIZE)),
    None => (1u64, page.size_within(PAGE_SIZE)),
  };
  let data = api
    .call("music.search.SearchCgiService", "DoSearchForQQMusicDesktop")
    .desktop()
    .arg("query", keyword)
    .arg("search_type", kind.code)
    .arg("page_num", number)
    .arg("num_per_page", size)
    .arg("grp", 1)
    .arg("remoteplace", "txt.newclient.top")
    .arg("searchid", search_id(kind.id_kind))
    .send()
    .await?;
  let items = data.list(&format!("body.{}", kind.items)).to_vec();
  let total = data.u64("meta.sum").unwrap_or(0);
  let more = data.i64("meta.nextpage").is_some_and(|n| n > 0)
    && !items.is_empty()
    && number * (size as u64) < total;
  Ok((items, more.then(|| format!("{}:{size}", number + 1))))
}

/// A search id as the web player makes them: the kind, a random part and
/// the milliseconds of the day.
fn search_id(kind: u64) -> String {
  let random = rand::random_range(0..=4_194_304u64);
  let day_ms = jiff::Timestamp::now()
    .as_millisecond()
    .rem_euclid(86_400_000) as u64;
  (kind * SEARCH_ID_KIND + random * SEARCH_ID_RANDOM + day_ms).to_string()
}
