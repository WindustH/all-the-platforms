//! Search, the hot boards and the feeds.

use atp_core::{Error, ErrorCode, Page, PageReq, Post, Query, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;

const SEARCH_VIDEO: &str = "/aweme/v1/web/search/item/";
const SEARCH_GENERAL: &str = "/aweme/v1/web/general/search/single/";
const SEARCH_USER: &str = "/aweme/v1/web/discover/search/";
const HOT: &str = "/aweme/v1/web/hot/search/list/";
const BILLBOARD: &str = "https://www.iesdouyin.com/web/api/v2/hotsearch/billboard/word/";
const RECOMMEND: &str = "/aweme/v1/web/tab/feed/";
const FOLLOWING: &str = "/aweme/v1/web/follow/feed/";
const FRIENDS: &str = "/aweme/v1/web/familiar/feed/";

pub const SEARCH_SORTS: &[&str] = &["general", "likes", "latest"];
pub const SEARCH_FILTERS: &[&str] = &[
  "day",
  "week",
  "half-year",
  "video",
  "images",
  "short",
  "medium",
  "long",
  "unseen",
  "following",
];
pub const HOT_CATEGORIES: &[&str] = &["all", "entertainment", "society", "challenge"];
pub const FEED_KINDS: &[&str] = &["recommend", "following", "friends"];

/// `offset` and, after the first page, the `search_id` Douyin wants back.
fn search_cursor(page: &PageReq) -> (u64, Option<String>) {
  match page.cursor.as_deref().and_then(|c| c.split_once('~')) {
    Some((offset, id)) => (offset.parse().unwrap_or(0), Some(id.to_owned())),
    None => (page.number_or(0), None),
  }
}

fn next_search(v: &Value, offset: u64, got: usize) -> Option<String> {
  if v.i64("has_more") != Some(1) || got == 0 {
    return None;
  }
  let next = v.u64("cursor").unwrap_or(offset + got as u64);
  let id = v.str("log_pb.impr_id").or_else(|| v.str("extra.logid"));
  Some(match id {
    Some(id) => format!("{next}~{id}"),
    None => next.to_string(),
  })
}

/// `filter_selected`: the sort plus at most one time / type / length /
/// range filter.
fn filters(query: &Query) -> Result<Option<String>> {
  let sort = match query.sort.as_deref() {
    None | Some("general") => "0",
    Some("likes") => "1",
    Some("latest") => "2",
    Some(other) => return Err(Error::input(format!("unknown sort: {other}"))),
  };
  let (mut time, mut kind, mut length, mut range) = ("0", "0", "", "0");
  match query.filter.as_deref() {
    None => {}
    Some("day") => time = "1",
    Some("week") => time = "7",
    Some("half-year") => time = "180",
    Some("video") => kind = "1",
    Some("images") => kind = "2",
    Some("short") => length = "0-1",
    Some("medium") => length = "1-5",
    Some("long") => length = "5-10000",
    Some("unseen") => range = "2",
    Some("following") => range = "3",
    Some(other) => return Err(Error::input(format!("unknown filter: {other}"))),
  }
  if sort == "0" && query.filter.is_none() {
    return Ok(None);
  }
  Ok(Some(
    json!({
      "sort_type": sort,
      "publish_time": time,
      "filter_duration": length,
      "search_range": range,
      "content_type": kind,
    })
    .to_string(),
  ))
}

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let (offset, search_id) = search_cursor(page);
  let filter = filters(query)?;
  // Image posts only show up in the general channel.
  let general = query.filter.as_deref() == Some("images");
  let mut call = api
    .get(if general {
      SEARCH_GENERAL
    } else {
      SEARCH_VIDEO
    })
    .arg(
      "search_channel",
      if general {
        "aweme_general"
      } else {
        "aweme_video_web"
      },
    )
    .arg("enable_history", "1")
    .arg("keyword", &query.keyword)
    .arg("search_source", "normal_search")
    .arg("query_correct_type", "1")
    .arg("is_filter_search", if filter.is_some() { "1" } else { "0" })
    .arg("from_group_id", "")
    .arg("disable_rs", "0")
    .arg("offset", offset)
    .arg("count", page.size_within(20))
    .arg("need_filter_settings", if offset == 0 { "1" } else { "0" })
    .arg("list_type", "single")
    .arg("search_id", search_id.unwrap_or_default());
  if let Some(f) = &filter {
    call = call.arg("filter_selected", f);
  }
  let v = call.send().await?;
  let items: Vec<Post> = v
    .list("data")
    .iter()
    .filter_map(|d| parse::aweme(d.at("aweme_info")))
    .collect();
  if items.is_empty()
    && let Some(kind) = v.str("search_nil_info.search_nil_type")
  {
    return Err(
      Error::new(
        ErrorCode::VerificationRequired,
        format!("Douyin returned no results ({kind})"),
      )
      .with_hint("search usually needs a logged-in session: `atp douyin login --browser`"),
    );
  }
  let next = next_search(&v, offset, items.len());
  Ok(Page::new(items, next))
}

pub async fn search_users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let (offset, search_id) = search_cursor(page);
  let v = api
    .get(SEARCH_USER)
    .arg("search_channel", "aweme_user_web")
    .arg("keyword", &query.keyword)
    .arg("search_source", "normal_search")
    .arg("query_correct_type", "1")
    .arg("is_filter_search", "0")
    .arg("from_group_id", "")
    .arg("offset", offset)
    .arg("count", page.size_within(20))
    .arg("need_filter_settings", if offset == 0 { "1" } else { "0" })
    .arg("list_type", "single")
    .arg("search_id", search_id.unwrap_or_default())
    .send()
    .await?;
  let items: Vec<User> = v
    .list("user_list")
    .iter()
    .filter_map(|u| parse::user(u.at("user_info")))
    .collect();
  let next = next_search(&v, offset, items.len());
  Ok(Page::new(items, next))
}

/// The hot boards. The signed web board carries view counts and covers;
/// when it is walled, the open billboard (words and heat) stands in.
pub async fn hot(api: &Api, category: Option<&str>, _page: &PageReq) -> Result<Page<Post>> {
  let (board, sub) = match category.unwrap_or("all") {
    "all" => ("0", ""),
    "entertainment" => ("2", "2"),
    "society" => ("2", "4"),
    "challenge" => ("2", "hotspot_challenge"),
    other => return Err(Error::input(format!("unknown hot category: {other}"))),
  };
  let web = api
    .get(HOT)
    .arg("detail_list", "1")
    .arg("source", "6")
    .arg("board_type", board)
    .arg("board_sub_type", sub)
    .arg("version_code", "170400")
    .arg("version_name", "17.4.0")
    .send()
    .await;
  let v = match web {
    Ok(v) => v,
    Err(e) if board == "0" => {
      tracing::debug!("hot board walled ({e}); using the open billboard");
      api.ctx.http.get(BILLBOARD).no_cookies().value().await?
    }
    Err(e) => return Err(e),
  };
  let words = match v.list("data.word_list") {
    [] => v.list("word_list"),
    w => w,
  };
  let items = words
    .iter()
    .enumerate()
    .filter_map(|(i, w)| parse::hot_word(w, i + 1))
    .collect();
  Ok(Page::last(items))
}

pub async fn feed(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let count = page.size_within(20);
  let pull_type = if page.cursor.is_none() { "0" } else { "2" };
  let v = match kind.unwrap_or("recommend") {
    "recommend" => return recommended(api, page).await,
    "following" => {
      api.require_login()?;
      api
        .get(FOLLOWING)
        .arg("cursor", page.number_or(0))
        .arg("level", "1")
        .arg("count", count)
        .arg("pull_type", pull_type)
        .send()
        .await?
    }
    "friends" => {
      api.require_login()?;
      api
        .get(FRIENDS)
        .arg("max_cursor", page.number_or(0))
        .arg("count", count)
        .arg("pull_type", pull_type)
        .send()
        .await?
    }
    other => return Err(Error::input(format!("unknown feed: {other}"))),
  };
  // Following / friends feeds wrap each aweme in a card.
  let items: Vec<Post> = v
    .list("data")
    .iter()
    .chain(v.list("aweme_list"))
    .filter_map(|d| parse::aweme(d.at("aweme")).or_else(|| parse::aweme(d)))
    .collect();
  let next = (v.i64("has_more") == Some(1))
    .then(|| v.first_str(&["cursor", "max_cursor"]))
    .flatten();
  Ok(Page::new(items, next))
}

/// The recommended feed, paged by refresh index.
async fn recommended(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let index = page.number_or(1);
  let v = api
    .get(RECOMMEND)
    .arg("count", page.size_within(20))
    .arg("video_type_select", "1")
    .arg("aweme_pc_rec_raw_data", r#"{"is_client":false}"#)
    .arg("globalwid", "")
    .arg("pull_type", if index == 1 { "0" } else { "2" })
    .arg("min_window", "0")
    .arg("free_right", "0")
    .arg("ug_source", "")
    .arg("creative_id", "")
    .arg("refresh_index", index)
    .send()
    .await?;
  let items: Vec<Post> = v
    .list("aweme_list")
    .iter()
    .filter_map(parse::aweme)
    .collect();
  let next = (!items.is_empty()).then(|| (index + 1).to_string());
  Ok(Page::new(items, next))
}
