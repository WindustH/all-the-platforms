//! The home feed, its channels, and the channels as hot lists.

use social_core::{Collection, Error, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::Api;
use crate::notes;
use crate::refs::SOURCE_FEED;

const HOMEFEED: &str = "/api/sns/web/v1/homefeed";

const CHANNELS: &str = "/api/sns/web/v1/homefeed/category";

/// `hot --category` values.
pub const HOT_CATEGORIES: &[&str] = &[
  "fashion",
  "food",
  "cosmetics",
  "movie",
  "career",
  "love",
  "home",
  "gaming",
  "travel",
  "fitness",
  "video",
];

/// `feed --type` values: the recommended feed, then the channels.
pub const FEED_KINDS: &[&str] = &[
  "recommend",
  "fashion",
  "food",
  "cosmetics",
  "movie",
  "career",
  "love",
  "home",
  "gaming",
  "travel",
  "fitness",
  "video",
];

const DEFAULT_HOT: &str = "food";

/// The homefeed channel of a hot category.
fn channel(category: &str) -> Option<&'static str> {
  Some(match category {
    "fashion" => "homefeed.fashion_v3",
    "food" => "homefeed.food_v3",
    "cosmetics" => "homefeed.cosmetics_v3",
    "movie" => "homefeed.movie_and_tv_v3",
    "career" => "homefeed.career_v3",
    "love" => "homefeed.love_v3",
    "home" => "homefeed.household_product_v3",
    "gaming" => "homefeed.gaming_v3",
    "travel" => "homefeed.travel_v3",
    "fitness" => "homefeed.fitness_v3",
    "video" => "homefeed.video_v3",
    _ => return None,
  })
}

/// The home page's channels, as `feed --type` / `hot --category` take them.
pub async fn channels(api: &Api) -> Result<Page<Collection>> {
  api.require_login()?;
  let data = api.get(CHANNELS, &[]).await?;
  let items = data
    .list("categories")
    .iter()
    .filter_map(|ch| {
      let id = ch.str("id")?;
      let kind = HOT_CATEGORIES
        .iter()
        .find(|k| channel(k) == Some(id.as_str()))
        .map(|k| (*k).to_owned());
      Some(Collection {
        name: ch.str("name").unwrap_or_default(),
        kind: "channel".into(),
        description: kind.map(|k| format!("feed --type {k}")),
        id,
        ..Collection::default()
      })
    })
    .collect();
  Ok(Page::last(items))
}

pub async fn feed(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  match kind {
    None | Some("recommend") => homefeed(api, "homefeed_recommend", page).await,
    Some(other) => hot(api, Some(other), page).await,
  }
}

pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let name = category.unwrap_or(DEFAULT_HOT);
  let channel = channel(name).ok_or_else(|| Error::input(format!("unknown category `{name}`")))?;
  homefeed(api, channel, page).await
}

/// One homefeed page; the cursor is `<cursor_score>|<note_index>`.
async fn homefeed(api: &Api, category: &str, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let cursor = page.cursor.as_deref().unwrap_or_default();
  let (score, index) = cursor.split_once('|').unwrap_or((cursor, "0"));
  let index: u64 = index.parse().unwrap_or(0);
  let num = page.size_within(40);
  let body = json!({
    "cursor_score": score,
    "num": num,
    "refresh_type": 1,
    "note_index": index,
    "unread_begin_note_id": "",
    "unread_end_note_id": "",
    "unread_note_count": 0,
    "category": category,
    "search_key": "",
    "need_num": num,
    "image_scenes": ["FD_PRV_WEBP", "FD_WM_WEBP"],
  });
  let data = api.post(HOMEFEED, &body).await?;
  let items = notes::listed(api, &data, SOURCE_FEED);
  let next = data
    .str("cursor_score")
    .filter(|_| !items.is_empty())
    .map(|s| format!("{s}|{}", index + items.len() as u64));
  Ok(Page::new(items, next))
}
