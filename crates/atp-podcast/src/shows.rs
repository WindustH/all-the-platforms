//! Finding shows and episodes: Apple's directory (search, charts) and
//! Podcast Index (search, trending); one show; a show's episodes from its
//! feed.

use atp_core::feed::{self, Feed};
use atp_core::paging::slice;
use atp_core::{Error, Page, PageReq, Post, Query, Result, Value, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Show};

pub const FILTERS: &[&str] = &["shows", "episodes", "index"];
pub const CHARTS: &[&str] = &["shows", "episodes", "trending"];
/// The most results one iTunes search returns.
const SEARCH_MAX: usize = 200;
/// How many rows of a chart are fetched.
const CHART_SIZE: usize = 100;
/// The most results one Podcast Index request returns.
const INDEX_MAX: usize = 1000;

/// iTunes search hands out no pages: ask for everything up to the end of
/// this page and keep the page.
pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let filter = query.filter.as_deref().unwrap_or("shows");
  if filter == "index" {
    return index_search(api, &query.keyword, page).await;
  }
  let episodes = filter == "episodes";
  let offset = page.number_or(0) as usize;
  let limit = (offset + page.size.max(1)).min(SEARCH_MAX);
  let entity = if episodes {
    "podcastEpisode"
  } else {
    "podcast"
  };
  let params = vec![
    ("term", query.keyword.clone()),
    ("media", "podcast".to_owned()),
    ("entity", entity.to_owned()),
    ("limit", limit.to_string()),
  ];
  let v = api.itunes("search", params).await?;
  let parse = if episodes {
    parse::apple_episode
  } else {
    parse::apple_show
  };
  let items: Vec<Post> = v.list("results").iter().map(parse).collect();
  // The page ends at `limit`; a full answer below Apple's maximum may go on.
  let more = items.len() >= limit && limit < SEARCH_MAX;
  Ok(Page::new(
    slice(items, page).items,
    more.then(|| limit.to_string()),
  ))
}

async fn index_search(api: &Api, words: &str, page: &PageReq) -> Result<Page<Post>> {
  let params = vec![
    ("q", words.to_owned()),
    ("max", page.size_within(INDEX_MAX).to_string()),
  ];
  let v = api.index("search/byterm", params).await?;
  Ok(Page::last(
    v.list("feeds").iter().map(parse::index_show).collect(),
  ))
}

/// Apple's top shows or episodes, or Podcast Index's trending shows.
pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let category = category.unwrap_or("shows");
  if category == "trending" {
    let params = vec![("max", page.size_within(INDEX_MAX).to_string())];
    let v = api.index("podcasts/trending", params).await?;
    return Ok(Page::last(
      v.list("feeds").iter().map(parse::index_show).collect(),
    ));
  }
  let episodes = category == "episodes";
  let kind = if episodes {
    "podcast-episodes"
  } else {
    "podcasts"
  };
  let v = api.chart(kind, CHART_SIZE).await?;
  let rows: Vec<Post> = v
    .list("feed.results")
    .iter()
    .map(|row| parse::chart_row(row, episodes))
    .collect();
  Ok(slice(rows, page))
}

/// What iTunes Lookup knows of an Apple show.
pub async fn apple_show(api: &Api, id: u64) -> Result<Value> {
  let params = vec![("id", id.to_string()), ("entity", "podcast".to_owned())];
  let v = api.itunes("lookup", params).await?;
  v.list("results")
    .iter()
    .find(|row| row.str("kind").as_deref() == Some("podcast"))
    .cloned()
    .ok_or_else(|| Error::not_found(format!("Apple Podcasts has no show {id}")))
}

/// The feed address of a show.
pub async fn feed_url(api: &Api, show: &Show) -> Result<String> {
  match show {
    Show::Feed(url) => Ok(url.clone()),
    Show::Apple(id) => apple_show(api, *id)
      .await?
      .str("feedUrl")
      .ok_or_else(|| Error::not_found(format!("Apple Podcasts lists no feed for show {id}"))),
  }
}

/// One show: Apple's listing completed from its feed (description,
/// website, episode count), or the feed alone.
pub async fn read(api: &Api, show: &Show) -> Result<Post> {
  match show {
    Show::Feed(url) => {
      let feed = feed::load(&api.ctx, url).await?;
      Ok(parse::feed_show(&feed, feed.url.clone()))
    }
    Show::Apple(id) => read_apple(api, *id).await,
  }
}

async fn read_apple(api: &Api, id: u64) -> Result<Post> {
  let listing = apple_show(api, id).await?;
  let mut post = parse::apple_show(&listing);
  let Some(url) = listing.str("feedUrl") else {
    return Ok(post);
  };
  // The listing stands on its own when the feed is gone or broken.
  if let Ok(feed) = feed::load(&api.ctx, &url).await {
    let from_feed = parse::feed_show(&feed, String::new());
    post.text = from_feed.text;
    for key in ["website", "episodes", "language"] {
      if let Some(value) = from_feed.extra.get(key) {
        post.extra.insert(key.into(), value.clone());
      }
    }
  }
  Ok(post)
}

/// A show's feed, loaded.
pub async fn load(api: &Api, arg: &str) -> Result<Feed> {
  let show = refs::show(arg)?;
  let url = feed_url(api, &show).await?;
  feed::load(&api.ctx, &url).await
}

/// A show's episodes from its feed, the newest first.
pub async fn episodes(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let feed = load(api, arg).await?;
  let mut items: Vec<Post> = feed
    .entries
    .iter()
    .map(|entry| parse::feed_episode(&feed, entry))
    .collect();
  items.sort_by_key(|post| std::cmp::Reverse(post.created_at));
  Ok(slice(items, page))
}
