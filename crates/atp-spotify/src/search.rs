//! Search the catalog: tracks, albums, artists, playlists, shows or
//! episodes. Since February 2026 Spotify answers at most ten items a page
//! to development-mode apps, and nothing past the thousandth result
//! (offset and limit together).

use atp_core::{Page, PageReq, Post, Query, Result, ValueExt};

use crate::api::Api;
use crate::parse;

/// `search --filter` values; the first is the default.
pub const FILTERS: &[&str] = &["track", "album", "artist", "playlist", "show", "episode"];
const PAGE: usize = 10;
const MAX_OFFSET: u64 = 1000;

pub async fn posts(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let kind = query.filter.as_deref().unwrap_or(FILTERS[0]);
  let offset = page.number_or(0);
  let limit = page.size_within(PAGE);
  let params = [
    ("q", query.keyword.clone()),
    ("type", kind.to_owned()),
    ("limit", limit.to_string()),
    ("offset", offset.to_string()),
  ];
  let v = api.get("/search", &params).await?;
  let found = v.at(&format!("{kind}s"));
  // Entries Spotify no longer has come as `null`.
  let items = found.list("items").iter().filter_map(parse::item).collect();
  let next = offset + limit as u64;
  let more = found.str("next").is_some() && next < MAX_OFFSET;
  Ok(Page::new(items, more.then(|| next.to_string())))
}
