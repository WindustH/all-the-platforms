//! Trending searches (热搜) and Zhihu Daily (知乎日报).

use atp_core::{Collection, Error, Page, PageReq, Post, Result, ValueExt};
use jiff::civil::Date;

use crate::api::{Api, V4};
use crate::parse;

const DAILY: &str = "https://news-at.zhihu.com/api/4/stories";

/// What people search for right now.
pub async fn hot_search(api: &Api) -> Result<Page<Collection>> {
  let v = api
    .call(api.get(&format!("{V4}/search/hot_search")))
    .await?;
  Ok(Page::last(
    v.list("hot_search_queries")
      .iter()
      .filter_map(parse::query)
      .collect(),
  ))
}

/// Daily stories, newest day first. `date` (`YYYYMMDD`) starts at that day;
/// the cursor is the day before the last one returned. No login needed.
pub async fn daily(api: &Api, date: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let url = match (&page.cursor, date) {
    (Some(day), _) => format!("{DAILY}/before/{day}"),
    (None, Some(day)) => format!("{DAILY}/before/{}", next_day(day)?),
    (None, None) => format!("{DAILY}/latest"),
  };
  let v = api.call_public(api.ctx.http.get(url).no_cookies()).await?;
  let day = v.str("date");
  let items = v
    .list("stories")
    .iter()
    .filter_map(|s| parse::daily_story(s, day.as_deref()))
    .collect();
  // Stories reach back to 2013-05-19; `before/<day>` lists the day before.
  Ok(Page::new(items, day.filter(|d| d.as_str() > "20130519")))
}

fn next_day(day: &str) -> Result<String> {
  let parsed = Date::strptime("%Y%m%d", day)
    .map_err(|_| Error::input(format!("`{day}` is not a date like 20260101")))?;
  let next = parsed.tomorrow().map_err(|e| Error::input(e.to_string()))?;
  Ok(next.strftime("%Y%m%d").to_string())
}
