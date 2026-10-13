//! Finding posts: search (through m.weibo.cn's search containers), the hot
//! lists (热搜 and its category boards) and the home feeds.

use social_core::{Collection, Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::Client;
use crate::{parse, people};

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

// ── search ────────────────────────────────────────────────────────────

/// One page of a m.weibo.cn search container (`100103type=<type>&q=<kw>`).
async fn search_cards(c: &Client, kind: &str, keyword: &str, page: u64) -> Result<Vec<Value>> {
  let container = format!("100103type={kind}&q={keyword}");
  let no = page.to_string();
  let v = c
    .get_mobile(
      "/api/container/getIndex",
      &[
        ("containerid", &container),
        ("page_type", "searchall"),
        ("page", &no),
      ],
    )
    .await?;
  Ok(v.list("data.cards").to_vec())
}

fn search_type(q: &Query) -> &'static str {
  match (q.filter.as_deref(), q.sort.as_deref()) {
    (Some("video"), _) => "64",
    (Some("image"), _) => "63",
    (_, Some("latest")) => "61",
    (_, Some("hot")) => "60",
    _ => "1",
  }
}

/// Search pages are numbered; an empty page ends the listing.
fn numbered<T>(items: Vec<T>, page: u64) -> Page<T> {
  let next = (!items.is_empty() && page < 50).then(|| (page + 1).to_string());
  Page::new(items, next)
}

pub async fn search(c: &Client, q: &Query, page: &PageReq) -> Result<Page<Post>> {
  let no = page.number_or(1);
  let cards = search_cards(c, search_type(q), &q.keyword, no).await?;
  Ok(numbered(parse::card_posts(&cards), no))
}

pub async fn search_users(c: &Client, q: &Query, page: &PageReq) -> Result<Page<User>> {
  let no = page.number_or(1);
  let cards = search_cards(c, "3", &q.keyword, no).await?;
  Ok(numbered(parse::card_users(&cards), no))
}

pub async fn search_topics(c: &Client, q: &Query, page: &PageReq) -> Result<Page<Collection>> {
  let no = page.number_or(1);
  let cards = search_cards(c, "38", &q.keyword, no).await?;
  Ok(numbered(parse::card_topics(&cards), no))
}

/// Posts under a hashtag (`#name#`), newest first.
pub async fn topic(c: &Client, name: &str, page: &PageReq) -> Result<Page<Post>> {
  let name = name.trim().trim_matches('#');
  if name.is_empty() {
    return Err(Error::input("empty topic"));
  }
  let no = page.number_or(1);
  let container = format!("231522type=1&t=10&q=#{name}#");
  let v = c
    .get_mobile(
      "/api/container/getIndex",
      &[
        ("containerid", &container),
        ("page_type", "searchall"),
        ("page", &no.to_string()),
      ],
    )
    .await?;
  Ok(numbered(parse::card_posts(v.list("data.cards")), no))
}

// ── hot ───────────────────────────────────────────────────────────────

pub async fn hot(c: &Client, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let path = match category.unwrap_or("realtime") {
    "realtime" => "/ajax/statuses/hot_band",
    "posts" => return feed(c, Some("hot"), page).await,
    "entertainment" => "/ajax/statuses/entertainment",
    "life" => "/ajax/statuses/life",
    "social" => "/ajax/statuses/social",
    "tech" => "/ajax/statuses/technology",
    "sports" => "/ajax/statuses/sport",
    "acg" => "/ajax/statuses/acg",
    "trend" => "/ajax/statuses/trendRank",
    "explode" => "/ajax/statuses/explode",
    "mine" => {
      c.require_login()?;
      "/ajax/statuses/mineBand"
    }
    other => return Err(Error::input(format!("unknown hot category: {other}"))),
  };
  let v = c.get(path, &[]).await?;
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

// ── feeds ─────────────────────────────────────────────────────────────

pub async fn feed(c: &Client, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let kind = kind.unwrap_or(if c.logged_in() { "following" } else { "hot" });
  let max_id = page.cursor.clone().unwrap_or_else(|| "0".into());
  let count = page.size_within(20).to_string();
  let v = match kind {
    "hot" => {
      c.get(
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
      c.require_login()?;
      let uid = people::own_uid(c).await?;
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
      c.get(path, &query).await?
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
