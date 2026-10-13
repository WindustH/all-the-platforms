//! The hot lists (热搜 and its category boards) and the home feeds.

use atp_core::{Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::Api;
use crate::{parse, users};

/// `hot --category` values: hot-search boards, plus `posts` for hot posts.
pub const HOT_CATEGORIES: &[&str] = &[
  "realtime",
  "posts",
  "entertainment",
  "life",
  "social",
  "tech",
  "sports",
  "acg",
  "trend",
  "explode",
  "mine",
];

pub const FEED_KINDS: &[&str] = &["following", "latest", "original", "video", "hot"];

pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let path = match category.unwrap_or("realtime") {
    "realtime" => "/ajax/statuses/hot_band",
    "posts" => return feed(api, Some("hot"), page).await,
    "entertainment" => "/ajax/statuses/entertainment",
    "life" => "/ajax/statuses/life",
    "social" => "/ajax/statuses/social",
    "tech" => "/ajax/statuses/technology",
    "sports" => "/ajax/statuses/sport",
    "acg" => "/ajax/statuses/acg",
    "trend" => "/ajax/statuses/trendRank",
    "explode" => "/ajax/statuses/explode",
    "mine" => {
      api.require_login()?;
      "/ajax/statuses/mineBand"
    }
    other => return Err(Error::input(format!("unknown hot category: {other}"))),
  };
  let v = api.get(path, &[]).await?;
  let data = v.at("data");
  let mut rows: Vec<&Value> = Vec::new();
  // The pinned government entry first, as the board shows it.
  if data.at("hotgov").is_object() {
    rows.push(data.at("hotgov"));
  }
  for key in ["band_list", "realtime", "list"] {
    rows.extend(data.list(key));
  }
  let items: Vec<Post> = rows
    .into_iter()
    .enumerate()
    .filter_map(|(i, v)| parse::trend(v, i))
    .collect();
  Ok(Page::last(items))
}

pub async fn feed(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let kind = kind.unwrap_or(if api.logged_in() { "following" } else { "hot" });
  let max_id = page.cursor.clone().unwrap_or_else(|| "0".into());
  let count = page.size_within(20).to_string();
  let v = match kind {
    "hot" => {
      api
        .get(
          "/ajax/feed/hottimeline",
          &[
            ("since_id", "0"),
            ("refresh", "0"),
            ("group_id", "102803"),
            ("containerid", "102803"),
            ("extparam", "discover|new_feed"),
            ("max_id", &max_id),
            ("count", &count),
          ],
        )
        .await?
    }
    "following" | "latest" | "original" | "video" => {
      api.require_login()?;
      let uid = users::own_uid(api).await?;
      let (path, prefix) = match kind {
        "following" => ("/ajax/feed/unreadfriendstimeline", "10001"),
        "latest" => ("/ajax/feed/friendstimeline", "11000"),
        "original" => ("/ajax/feed/friendstimeline", "11001"),
        _ => ("/ajax/feed/friendstimeline", "11002"),
      };
      let list = format!("{prefix}{uid}");
      let mut query = vec![
        ("list_id", list.as_str()),
        ("refresh", "4"),
        ("since_id", "0"),
        ("max_id", max_id.as_str()),
        ("count", count.as_str()),
      ];
      if prefix != "10001" {
        query.push(("fid", list.as_str()));
      }
      api.get(path, &query).await?
    }
    other => return Err(Error::input(format!("unknown feed: {other}"))),
  };
  Ok(timeline(&v))
}

/// A timeline response (`statuses` + `max_id`).
pub fn timeline(v: &Value) -> Page<Post> {
  let items: Vec<Post> = v.list("statuses").iter().filter_map(parse::post).collect();
  let next = v
    .first_str(&["max_id_str", "max_id"])
    .filter(|m| m != "0" && !m.is_empty() && !items.is_empty());
  Page::new(items, next)
}
