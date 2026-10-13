//! The data center's lists: your notes with their numbers, and your most
//! active fans.

use social_core::{Page, PageReq, Post, Result, User, ValueExt};

use super::{metrics, window};
use crate::api::Api;
use crate::parse;

/// 内容分析 note list (`NOTE_ANALYZE_LIST`, called by chunk 4323).
const NOTE_STATS: &str = "/api/galaxy/creator/datacenter/note/analyze/list";

/// 我的活跃粉丝 (`ACTIVE_FANS_NEW`, called by chunk 7763).
const ACTIVE_FANS: &str = "/api/galaxy/creator/data/active_fans_new";

/// Your notes with their numbers, newest first, 10 per page like the page.
pub async fn note_stats(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let (page, size) = (req.number_or(1), 10);
  let page_s = page.to_string();
  let params = [
    ("type", "0"),
    ("page_size", "10"),
    ("page_num", page_s.as_str()),
  ];
  let data = parse::snake_keys(api.creator_get(NOTE_STATS, &params).await?);
  let items: Vec<Post> = data
    .list("note_infos")
    .iter()
    .filter_map(metrics::note_row)
    .collect();
  let more = !items.is_empty()
    && match data.u64("total") {
      Some(total) => page * size < total,
      None => items.len() as u64 >= size,
    };
  Ok(Page::new(items, more.then(|| (page + 1).to_string())))
}

/// Fans who interacted most over the last 7 or 30 days.
pub async fn active_fans(api: &Api, days: u32) -> Result<Page<User>> {
  api.require_login()?;
  let data = parse::snake_keys(api.creator_get(ACTIVE_FANS, &[]).await?);
  let rows = data.list(window(days).0);
  Ok(Page::last(
    rows.iter().filter_map(metrics::active_fan).collect(),
  ))
}
