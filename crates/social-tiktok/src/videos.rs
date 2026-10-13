//! Videos and photo posts: reading one, comments and replies, search, the
//! recommended feed, related videos, and the videos of a hashtag, a sound or
//! a playlist.

use social_core::{
  Collection, Comment, Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt,
};

use crate::api::{Api, WWW, node, status_error};
use crate::{parse, refs};

/// Query parameters of an endpoint.
pub type Params = Vec<(&'static str, String)>;

/// Detail nodes of the page data that hold one item.
const DETAIL_KEYS: &[&str] = &[
  "webapp.video-detail",
  "webapp.photo-detail",
  "webapp.reflow.video.detail",
];

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let r = refs::post_ref(api, arg).await?;
  // A first visit (no cookies yet) is sometimes served without the item.
  for _ in 0..2 {
    let scope = api.page(&r.url()).await?;
    if let Some(detail) = DETAIL_KEYS
      .iter()
      .map(|k| node(&scope, k))
      .find(|d| d.is_object())
    {
      let code = detail.i64("statusCode").unwrap_or(0);
      if code != 0 {
        let msg = detail.str("statusMsg").unwrap_or_default();
        return Err(status_error(&format!("video {}", r.id), code, &msg));
      }
      if let Some(post) = parse::item(detail.at("itemInfo.itemStruct")) {
        return Ok(post);
      }
    }
  }
  // Pages served without the item (stricter networks): the API has it.
  let v = api
    .get("/api/item/detail/", &[("itemId", r.id.clone())])
    .await?;
  parse::item(v.at("itemInfo.itemStruct"))
    .ok_or_else(|| Error::not_found(format!("no video {}", r.id)))
}

/// A cursor-paged item list: `itemList` / `item_list`, `hasMore`, `cursor`.
pub async fn item_page(
  api: &Api,
  path: &str,
  mut params: Params,
  req: &PageReq,
  max: usize,
) -> Result<Page<Post>> {
  params.push(("count", req.size_within(max).to_string()));
  params.push(("cursor", req.cursor.clone().unwrap_or_else(|| "0".into())));
  let v = api.get(path, &params).await?;
  let items: Vec<Post> = ["itemList", "item_list"]
    .iter()
    .flat_map(|k| v.list(k))
    .filter_map(parse::item)
    .collect();
  Ok(Page::new(items, next_cursor(&v)))
}

/// The cursor to continue with, when the answer says there is more.
pub fn next_cursor(v: &Value) -> Option<String> {
  let more = v.bool("hasMore").unwrap_or(false) || v.i64("has_more").is_some_and(|n| n != 0);
  more.then(|| v.str("cursor")).flatten()
}

pub async fn comments(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Comment>> {
  let r = refs::post_ref(api, arg).await?;
  let params = vec![
    ("aweme_id", r.id),
    ("count", req.size_within(50).to_string()),
    ("cursor", req.cursor.clone().unwrap_or_else(|| "0".into())),
    ("enter_from", "tiktok_web".into()),
    ("from_page", "video".into()),
  ];
  let v = api.get("/api/comment/list/", &params).await?;
  let items = v.list("comments").iter().map(parse::comment).collect();
  Ok(Page::new(items, next_cursor(&v)))
}

pub async fn replies(api: &Api, post: &str, comment: &str, req: &PageReq) -> Result<Page<Comment>> {
  let r = refs::post_ref(api, post).await?;
  let params = vec![
    ("item_id", r.id),
    ("comment_id", comment.trim().to_owned()),
    ("count", req.size_within(50).to_string()),
    ("cursor", req.cursor.clone().unwrap_or_else(|| "0".into())),
  ];
  let v = api.get("/api/comment/list/reply/", &params).await?;
  let items = v.list("comments").iter().map(parse::comment).collect();
  Ok(Page::new(items, next_cursor(&v)))
}

/// Search cursors carry the upstream offset and the search id: `offset|search_id`.
fn search_cursor(req: &PageReq) -> (String, Option<String>) {
  match req.cursor.as_deref().and_then(|c| c.split_once('|')) {
    Some((offset, id)) => (
      offset.to_owned(),
      Some(id.to_owned()).filter(|s| !s.is_empty()),
    ),
    None => (req.cursor.clone().unwrap_or_else(|| "0".into()), None),
  }
}

fn search_next(v: &Value) -> Option<String> {
  next_cursor(v).map(|c| {
    let id = v
      .first_str(&["rid", "log_pb.impr_id", "extra.logid"])
      .unwrap_or_default();
    format!("{c}|{id}")
  })
}

async fn search_raw(api: &Api, kind: &str, keyword: &str, req: &PageReq) -> Result<Value> {
  let (offset, search_id) = search_cursor(req);
  let mut params: Params = vec![
    ("keyword", keyword.to_owned()),
    ("offset", offset.clone()),
    ("cursor", offset),
    ("count", req.size_within(20).to_string()),
    ("from_page", "search".into()),
  ];
  if let Some(id) = search_id {
    params.push(("search_id", id));
  }
  api.get(&format!("/api/search/{kind}/full/"), &params).await
}

/// Videos for a keyword: the "Top" tab (`general`, default) or `video`.
pub async fn search(api: &Api, q: &Query, req: &PageReq) -> Result<Page<Post>> {
  let video_only = q.filter.as_deref() == Some("video");
  let v = search_raw(
    api,
    if video_only { "item" } else { "general" },
    &q.keyword,
    req,
  )
  .await?;
  let items: Vec<Post> = if video_only {
    v.list("item_list").iter().filter_map(parse::item).collect()
  } else {
    v.list("data")
      .iter()
      .filter_map(|d| parse::item(d.at("item")))
      .collect()
  };
  Ok(Page::new(items, search_next(&v)))
}

pub async fn search_users(api: &Api, q: &Query, req: &PageReq) -> Result<Page<User>> {
  let v = search_raw(api, "user", &q.keyword, req).await?;
  let items = v.list("user_list").iter().map(parse::listed_user).collect();
  Ok(Page::new(items, search_next(&v)))
}

/// Hashtags for a keyword: the exact hashtag (TikTok's web search has no
/// hashtag tab), with its video and view counts.
pub async fn search_hashtags(api: &Api, q: &Query, _req: &PageReq) -> Result<Page<Collection>> {
  let name = q.keyword.trim().trim_start_matches('#').replace(' ', "");
  match hashtag(api, &name).await {
    Ok(tag) => Ok(Page::last(vec![tag])),
    Err(e) if e.code == social_core::ErrorCode::NotFound => Ok(Page::last(Vec::new())),
    Err(e) => Err(e),
  }
}

/// The recommended feed ("For You"; personal when logged in).
pub async fn recommended(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  let params = vec![
    ("from_page", "fyp".into()),
    (
      "pullType",
      if req.cursor.is_some() { "2" } else { "1" }.into(),
    ),
    ("itemID", "1".into()),
  ];
  let mut page = item_page(api, "/api/recommend/item_list/", params, req, 30).await?;
  // The feed is endless; its cursor does not name a position, any value continues it.
  if page.next_cursor.is_none() && !page.items.is_empty() {
    page.next_cursor = Some("1".into());
    page.has_more = true;
  }
  Ok(page)
}

pub async fn related(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let r = refs::post_ref(api, arg).await?;
  let params = vec![("itemID", r.id), ("from_page", "video".into())];
  let mut page = item_page(api, "/api/related/item_list/", params, req, 16).await?;
  page.next_cursor = None;
  page.has_more = false;
  Ok(page)
}

// ── hashtags, sounds, playlists ─────────────────────────────────────────

pub async fn hashtag(api: &Api, arg: &str) -> Result<Collection> {
  let name = refs::tag_name(arg)?;
  let scope = api
    .page(&format!("{WWW}/tag/{}", crate::api::encode(&name)))
    .await?;
  let info = node(&scope, "webapp.challenge-detail").at("challengeInfo");
  if let Some(tag) = parse::hashtag(info).filter(|t| !t.id.is_empty()) {
    return Ok(tag);
  }
  // Pages often come without the hashtag node; the API has it.
  let v = api
    .get("/api/challenge/detail/", &[("challengeName", name.clone())])
    .await?;
  parse::hashtag(v.at("challengeInfo"))
    .filter(|t| !t.id.is_empty())
    .ok_or_else(|| Error::not_found(format!("no hashtag #{name}")))
}

pub async fn hashtag_videos(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let id = if refs::is_id(arg.trim()) {
    arg.trim().to_owned()
  } else {
    hashtag(api, arg).await?.id
  };
  item_page(
    api,
    "/api/challenge/item_list/",
    vec![("challengeID", id)],
    req,
    30,
  )
  .await
}

pub async fn sound(api: &Api, arg: &str) -> Result<Collection> {
  let id = refs::music_id(arg)?;
  let scope = api.page(&format!("{WWW}/music/x-{id}")).await?;
  if let Some(s) = parse::sound(node(&scope, "webapp.music-detail").at("musicInfo")) {
    return Ok(s);
  }
  let v = api
    .get("/api/music/detail/", &[("musicId", id.clone())])
    .await?;
  parse::sound(v.at("musicInfo")).ok_or_else(|| Error::not_found(format!("no sound {id}")))
}

pub async fn sound_videos(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let id = refs::music_id(arg)?;
  item_page(api, "/api/music/item_list/", vec![("musicID", id)], req, 30).await
}

pub async fn playlist(api: &Api, arg: &str) -> Result<Collection> {
  let id = refs::playlist_id(arg)?;
  let v = api
    .get("/api/mix/detail/", &[("mixId", id.clone())])
    .await?;
  parse::folder(v.at("mixInfo"), "playlist")
    .ok_or_else(|| Error::not_found(format!("no playlist {id}")))
}

pub async fn playlist_videos(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let id = refs::playlist_id(arg)?;
  item_page(api, "/api/mix/item_list/", vec![("mixId", id)], req, 30).await
}
