//! Search (the desktop client's `cloudsearch`, which answers without a
//! login): songs, albums, artists, playlists, podcasts and users.

use atp_core::{Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse;

/// `search --filter` values; the first is the default.
pub const FILTERS: &[&str] = &["song", "album", "artist", "playlist", "podcast"];

/// One kind of result: the upstream type, where the items are, how they
/// are counted.
struct Kind {
  code: u32,
  items: &'static str,
  count: &'static str,
}

fn kind(filter: Option<&str>) -> Kind {
  let (code, items, count) = match filter.unwrap_or(FILTERS[0]) {
    "album" => (10, "albums", "albumCount"),
    "artist" => (100, "artists", "artistCount"),
    "playlist" => (1000, "playlists", "playlistCount"),
    "podcast" => (1009, "djRadios", "djRadiosCount"),
    "user" => (1002, "userprofiles", "userprofileCount"),
    _ => (1, "songs", "songCount"),
  };
  Kind { code, items, count }
}

pub async fn posts(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let filter = query.filter.as_deref();
  let parse: fn(&Value) -> Option<Post> = match filter.unwrap_or(FILTERS[0]) {
    "album" => parse::album,
    "artist" => parse::artist,
    "playlist" => parse::playlist,
    "podcast" => parse::podcast,
    _ => parse::song,
  };
  let (items, next) = run(api, &query.keyword, kind(filter), page).await?;
  Ok(Page::new(items.iter().filter_map(parse).collect(), next))
}

pub async fn users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let (items, next) = run(api, &query.keyword, kind(Some("user")), page).await?;
  Ok(Page::new(
    items.iter().filter_map(parse::user).collect(),
    next,
  ))
}

/// The items of one page and the next offset.
async fn run(
  api: &Api,
  keyword: &str,
  kind: Kind,
  page: &PageReq,
) -> Result<(Vec<Value>, Option<String>)> {
  let offset = page.number_or(0);
  let limit = page.size_within(50) as u64;
  let v = api
    .client("cloudsearch/pc")
    .arg("s", keyword)
    .arg("type", kind.code)
    .arg("limit", limit)
    .arg("offset", offset)
    .arg("total", true)
    .send()
    .await?;
  let result = v.at("result");
  let items = result.list(kind.items).to_vec();
  let total = result.u64(kind.count).unwrap_or(0);
  let end = offset + items.len() as u64;
  let next = (!items.is_empty() && end < total).then(|| end.to_string());
  Ok((items, next))
}
