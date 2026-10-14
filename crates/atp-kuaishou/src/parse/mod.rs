//! Kuaishou payloads → the shared model.
//!
//! A photo (作品) is a video or an image set (`atlas`). Feeds and detail
//! pages wrap it as `{photo, author, tags}`; the mobile share page carries
//! a flat photo with `userEid` / `userName`. Videos come as a `manifest` of
//! representations; the best one is the largest, then the fastest.

mod live;
mod users;

pub use self::live::*;
pub use self::users::*;

use atp_core::text::from_millis;
use atp_core::{Comment, Extra, Media, Metrics, Post, User, Value, ValueExt, json};

use crate::refs::{photo_url, profile_url, search_url};

/// Downloads from the video CDNs want the site as referer.
const MEDIA_REFERER: &str = "https://www.kuaishou.com/";

fn put(extra: &mut Extra, key: &str, value: Option<Value>) {
  if let Some(v) = value.filter(|v| !v.is_null() && v != &json!("")) {
    extra.insert(key.into(), v);
  }
}

/// A list cursor that says there is more (`no_more` or empty at the end).
pub fn more(cursor: Option<String>) -> Option<String> {
  cursor.filter(|c| !matches!(c.as_str(), "no_more" | "-1" | "None" | "null" | "undefined"))
}

/// A feed entry (`{photo, author, tags}`), a detail, or a bare photo.
pub fn post(v: &Value) -> Option<Post> {
  let photo = if v.at("photo").is_object() {
    v.at("photo")
  } else {
    v
  };
  let id = photo.first_str(&["id", "photoId", "photo_id"])?;
  let media = media(photo);
  let images = !atlas(photo).is_empty();
  let author = if v.at("author").is_object() {
    author(v.at("author"))
  } else {
    share_author(photo)
  };
  Some(Post {
    url: Some(photo_url(&id)),
    kind: if images { "images" } else { "video" }.into(),
    text: photo.first_str(&["caption", "originCaption"]),
    author,
    created_at: photo.i64("timestamp").and_then(from_millis),
    metrics: metrics(photo),
    media,
    tags: v
      .list("tags")
      .iter()
      .filter_map(|t| t.str("name"))
      .collect(),
    extra: photo_extra(photo),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  })
}

fn metrics(photo: &Value) -> Metrics {
  Metrics {
    views: photo.first_count(&["viewCount", "view_count"]),
    likes: photo.first_count(&["realLikeCount", "likeCount", "like_count"]),
    comments: photo.first_count(&["commentCount", "comment_count"]),
    shares: photo.first_count(&["shareCount", "forwardCount"]),
    favorites: photo.first_count(&["collectCount", "collect_count"]),
    ..Metrics::default()
  }
}

/// Cover, duration and the `expTag` likes and comments send back.
fn photo_extra(photo: &Value) -> Extra {
  let mut extra = Extra::new();
  let cover = photo.first_str(&["coverUrl", "coverUrls.0.url", "webpCoverUrls.0.url"]);
  put(&mut extra, "cover", cover.map(Value::from));
  put(
    &mut extra,
    "duration_ms",
    photo.u64("duration").map(Value::from),
  );
  put(
    &mut extra,
    "exp_tag",
    photo.first_str(&["expTag", "exp_tag"]).map(Value::from),
  );
  extra
}

/// Image URLs of an image set: `atlas` / `ext_params.atlas` `{cdn, list}`.
fn atlas(photo: &Value) -> Vec<String> {
  let atlas = [photo.at("atlas"), photo.at("ext_params.atlas")]
    .into_iter()
    .find(|a| a.is_object());
  let Some(atlas) = atlas else {
    return Vec::new();
  };
  let cdn = atlas.first_str(&["cdn", "cdn.0", "cdnList.0.cdn"]);
  atlas
    .list("list")
    .iter()
    .filter_map(Value::as_str)
    .map(|path| match &cdn {
      Some(cdn) if !path.starts_with("http") => format!("https://{cdn}{path}"),
      _ => path.to_owned(),
    })
    .collect()
}

/// The images of an image set, or the best video.
pub fn media(photo: &Value) -> Vec<Media> {
  let images = atlas(photo);
  if !images.is_empty() {
    return images
      .into_iter()
      .map(|url| referred(Media::image(url)))
      .collect();
  }
  let duration = photo.f64("duration").map(|ms| ms / 1000.0);
  let manifest = match photo.at("manifest") {
    Value::String(s) => serde_json::from_str(s).unwrap_or_default(),
    v => v.clone(),
  };
  let (url, size) = match best(&manifest) {
    Some(r) => (r.str("url"), Some(r)),
    None => (
      photo.first_str(&["photoUrl", "photoUrls.0.url", "mainMvUrls.0.url"]),
      None,
    ),
  };
  let Some(url) = url else {
    return Vec::new();
  };
  let mut video = referred(Media::video(url));
  let dims = size.unwrap_or(photo);
  video.width = dims.u64("width").map(|w| w as u32);
  video.height = dims.u64("height").map(|h| h as u32);
  video.duration = duration;
  vec![video]
}

fn referred(mut media: Media) -> Media {
  media.headers.push(("referer".into(), MEDIA_REFERER.into()));
  media
}

/// The largest representation of a manifest, then the highest bitrate.
fn best(manifest: &Value) -> Option<&Value> {
  manifest
    .list("adaptationSet")
    .iter()
    .flat_map(|set| set.list("representation"))
    .filter(|r| r.str("url").is_some())
    .max_by_key(|r| {
      let area = r.u64("width").unwrap_or(0) * r.u64("height").unwrap_or(0);
      let rate = r
        .first_count(&["maxBitrate", "avgBitrate", "bitrate"])
        .unwrap_or(0);
      (area, rate)
    })
}

/// A hot-board word: its rank, heat, cover and the photos about it.
pub fn hot(item: &Value) -> Option<Post> {
  let name = item.first_str(&["name", "id"])?;
  let mut post = Post {
    id: name.clone(),
    kind: "hot".into(),
    title: Some(name.clone()),
    url: Some(search_url(&name)),
    raw: Some(item.clone()),
    ..Post::default()
  };
  if let Some(heat) = item.count("hotValue") {
    post.metrics.other.insert("heat".into(), heat);
  }
  post.metrics.views = item.count("viewCount");
  put(&mut post.extra, "rank", item.u64("rank").map(Value::from));
  put(
    &mut post.extra,
    "cover",
    item.str("poster").map(Value::from),
  );
  put(&mut post.extra, "tag", item.str("tagType").map(Value::from));
  let photos = item.at("photoIds.json");
  if photos.is_array() {
    post.extra.insert("photo_ids".into(), photos.clone());
  }
  Some(post)
}

/// A comment (`rootCommentsV2` / `subCommentsV2` and the older fields),
/// with its inline replies.
pub fn comment(v: &Value, parent: Option<&str>) -> Option<Comment> {
  let id = v.first_str(&["commentId", "comment_id"])?;
  let author = v
    .first_str(&["authorId", "author_id", "authorEid"])
    .map(|aid| User {
      url: Some(profile_url(&aid)),
      id: aid,
      name: v
        .first_str(&["authorName", "author_name"])
        .unwrap_or_default(),
      avatar: v.str("headurl"),
      ..User::default()
    });
  let replies = v
    .list("subComments")
    .iter()
    .filter_map(|r| comment(r, Some(&id)))
    .collect();
  let mut c = Comment {
    author,
    text: v.str("content").unwrap_or_default(),
    created_at: v.i64("timestamp").and_then(from_millis),
    likes: v.first_count(&["realLikedCount", "likedCount"]),
    reply_count: v.count("subCommentCount"),
    reply_to: parent.map(str::to_owned),
    replies,
    raw: Some(v.clone()),
    id,
    ..Comment::default()
  };
  put(
    &mut c.extra,
    "has_replies",
    v.bool("hasSubComments").map(Value::from),
  );
  put(
    &mut c.extra,
    "reply_to_user",
    v.str("replyToUserName").map(Value::from),
  );
  Some(c)
}

/// The JSON object a page assigns after `marker` (`window.X = {...}`).
pub fn page_state(html: &str, marker: &str) -> Option<Value> {
  let start = html.find(marker)? + marker.len();
  let rest = html[start..].trim_start();
  let rest = rest.strip_prefix('=').unwrap_or(rest).trim_start();
  let mut values = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
  values.next()?.ok()
}

/// The photo object of the mobile share page's `INIT_STATE` (its keys are
/// scrambled; the value holding `photo` is the one).
pub fn share_photo(state: &Value) -> Option<&Value> {
  state
    .as_object()?
    .values()
    .find(|v| v.at("photo").is_object())
    .map(|v| v.at("photo"))
}

/// A row of the creator center's works list, with its review state.
pub fn work(v: &Value) -> Option<Post> {
  let id = v.first_str(&["workId", "photoId", "photoIdStr", "publishId"])?;
  let images = v.str("atlasId").is_some()
    || v.i64("photoType") == Some(1)
    || v.bool("showAtlasIcon") == Some(true);
  let mut post = Post {
    url: v
      .first_str(&["workId", "photoId", "photoIdStr"])
      .map(|w| photo_url(&w)),
    kind: if images { "images" } else { "video" }.into(),
    text: v.first_str(&["caption", "title"]),
    created_at: ["publishTime", "uploadTime", "createTime"]
      .iter()
      .find_map(|k| v.i64(k))
      .and_then(from_millis),
    metrics: Metrics {
      views: v.first_count(&["playCount", "viewCount"]),
      likes: v.count("likeCount"),
      comments: v.count("commentCount"),
      shares: v.count("shareCount"),
      favorites: v.count("collectCount"),
      ..Metrics::default()
    },
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  put(&mut post.extra, "status", work_status(v).map(Value::from));
  let cover = v.first_str(&["coverUrl", "publishCoverUrl", "cover"]);
  put(&mut post.extra, "cover", cover.map(Value::from));
  Some(post)
}

/// `published`, `reviewing`, `private` or `rejected`.
fn work_status(v: &Value) -> Option<&'static str> {
  let status = v.first_str(&["publishStatus", "status", "auditStatus"])?;
  let status: i64 = status.parse().ok()?;
  if v.i64("photoStatus") == Some(2) || v.i64("privacy") == Some(2) {
    return Some("private");
  }
  match status {
    1 | 3 => Some("published"),
    0 | 2 => Some("reviewing"),
    4 => Some("private"),
    s if s < 0 => Some("rejected"),
    _ => None,
  }
}
