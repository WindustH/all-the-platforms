//! TikTok payloads → the shared model. Items (videos and photo posts) and
//! users come in camelCase from pages and most API lists; comments and
//! search results in snake_case. Counters are read from `statsV2` (exact,
//! as strings) before `stats` (rounded on some pages).

use social_core::text::from_secs;
use social_core::{
  Collection, Comment, Media, MediaKind, Metrics, Post, User, UserStats, Value, ValueExt, json,
};

use crate::refs::{user_url, video_url};

/// A counter from `statsV2` (exact) or `stats`.
fn stat(item: &Value, v2: &str, v1: &str, key: &str) -> Option<u64> {
  item
    .count(&format!("{v2}.{key}"))
    .or_else(|| item.count(&format!("{v1}.{key}")))
}

pub fn item(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let author = v.at("author");
  let handle = author
    .first_str(&["uniqueId", "unique_id"])
    .unwrap_or_default();
  let photo = !v.at("imagePost").is_null();
  let metric = |key: &str| stat(v, "statsV2", "stats", key);
  let mut metrics = Metrics {
    views: metric("playCount"),
    likes: metric("diggCount"),
    comments: metric("commentCount"),
    shares: metric("shareCount"),
    favorites: metric("collectCount"),
    ..Metrics::default()
  };
  if let Some(n) = metric("repostCount") {
    metrics.other.insert("reposts".into(), n);
  }
  let tags = v
    .list("textExtra")
    .iter()
    .filter_map(|t| t.str("hashtagName"))
    .chain(v.list("challenges").iter().filter_map(|c| c.str("title")))
    .fold(Vec::new(), |mut acc: Vec<String>, t| {
      if !acc.iter().any(|a| a.eq_ignore_ascii_case(&t)) {
        acc.push(t);
      }
      acc
    });
  let mut extra = social_core::Extra::new();
  let mut put = |k: &str, value: Option<Value>| {
    if let Some(value) = value.filter(|x| !x.is_null()) {
      extra.insert(k.into(), value);
    }
  };
  let music = v.at("music");
  put(
    "music",
    music.str("id").map(|id| {
      json!({
        "id": id,
        "title": music.str("title"),
        "author": music.str("authorName"),
        "original": music.bool("original"),
        "url": music.str("playUrl"),
      })
    }),
  );
  put("duration", v.at("video.duration").as_u64().map(Value::from));
  put(
    "cover",
    v.first_str(&["video.originCover", "video.cover"])
      .map(Value::from),
  );
  put(
    "location_created",
    v.str("locationCreated").map(Value::from),
  );
  put("poi", v.str("poi.name").map(Value::from));
  put("is_ad", v.bool("isAd").filter(|b| *b).map(Value::from));
  put(
    "ai_generated",
    v.bool("IsAigc").filter(|b| *b).map(Value::from),
  );
  put("liked", v.bool("digged").map(Value::from));
  put("favorited", v.bool("collected").map(Value::from));
  put(
    "pinned",
    v.bool("isPinnedItem").filter(|b| *b).map(Value::from),
  );
  put("sec_uid", author.str("secUid").map(Value::from));
  Some(Post {
    url: Some(video_url(&handle, &id, photo)),
    kind: if photo { "photo" } else { "video" }.into(),
    title: v.str("imagePost.title"),
    text: v.str("desc"),
    author: (!author.is_null()).then(|| user_brief(author)),
    created_at: v.i64("createTime").and_then(from_secs),
    metrics,
    media: media(v),
    tags,
    extra,
    raw: Some(v.clone()),
    id,
    ..Post::default()
  })
}

/// The media of an item: the best stream of a video, or the images (and the
/// sound) of a photo post.
pub fn media(v: &Value) -> Vec<Media> {
  let images = v.list("imagePost.images");
  if !images.is_empty() {
    let mut out: Vec<Media> = images
      .iter()
      .filter_map(|img| {
        let url = img.str("imageURL.urlList.0")?;
        let mut m = Media::image(url);
        m.width = img.u64("imageWidth").map(|n| n as u32);
        m.height = img.u64("imageHeight").map(|n| n as u32);
        Some(m)
      })
      .collect();
    if let Some(url) = v.str("music.playUrl") {
      let mut m = Media::new(MediaKind::Audio, url);
      m.duration = v.f64("music.duration");
      out.push(m);
    }
    return out;
  }
  best_video(v.at("video")).into_iter().collect()
}

/// The stream with the most pixels, then the highest bitrate; the plain play
/// address when the item lists no ladder.
fn best_video(video: &Value) -> Option<Media> {
  let duration = video.f64("duration");
  let best = video
    .list("bitrateInfo")
    .iter()
    .filter_map(|b| {
      let addr = b.at("PlayAddr");
      let url = addr.str("UrlList.0")?;
      let (w, h) = (
        addr.u64("Width").unwrap_or(0),
        addr.u64("Height").unwrap_or(0),
      );
      Some((w * h, b.u64("Bitrate").unwrap_or(0), url, addr, w, h))
    })
    .max_by_key(|(pixels, bitrate, ..)| (*pixels, *bitrate));
  let mut m = match best {
    Some((_, _, url, addr, w, h)) => {
      let mut m = Media::video(url);
      m.width = (w > 0).then_some(w as u32);
      m.height = (h > 0).then_some(h as u32);
      m.size = addr.u64("DataSize");
      m
    }
    None => {
      let url = video.first_str(&["playAddr", "downloadAddr"])?;
      let mut m = Media::video(url);
      m.width = video.u64("width").map(|n| n as u32);
      m.height = video.u64("height").map(|n| n as u32);
      m
    }
  };
  m.duration = duration;
  Some(m)
}

/// A user as items and comments embed them (camelCase or snake_case).
pub fn user_brief(u: &Value) -> User {
  let handle = u.first_str(&["uniqueId", "unique_id"]);
  User {
    id: u.first_str(&["id", "uid", "user_id"]).unwrap_or_default(),
    name: u
      .str("nickname")
      .or_else(|| handle.clone())
      .unwrap_or_default(),
    url: handle.as_deref().map(user_url),
    avatar: u.first_str(&[
      "avatarLarger",
      "avatarMedium",
      "avatarThumb",
      "avatar_larger.url_list.0",
      "avatar_medium.url_list.0",
      "avatar_thumb.url_list.0",
    ]),
    bio: u.str("signature").filter(|s| !s.trim().is_empty()),
    verified: u.bool("verified").unwrap_or(false) || u.i64("custom_verify").is_some_and(|n| n > 0),
    extra: sec_uid_extra(u),
    raw: Some(u.clone()),
    handle,
    ..User::default()
  }
}

fn sec_uid_extra(u: &Value) -> social_core::Extra {
  let mut extra = social_core::Extra::new();
  if let Some(s) = u.first_str(&["secUid", "sec_uid"]) {
    extra.insert("sec_uid".into(), s.into());
  }
  if let Some(true) = u.bool("privateAccount") {
    extra.insert("private".into(), true.into());
  }
  if let Some(r) = u.str("region") {
    extra.insert("region".into(), r.into());
  }
  extra
}

/// A profile: `userInfo` of a page or of `/api/user/detail/`.
pub fn user(info: &Value) -> User {
  let u = info.at("user");
  let stat = |key: &str| stat(info, "statsV2", "stats", key);
  let mut stats = UserStats {
    followers: stat("followerCount"),
    following: stat("followingCount"),
    posts: stat("videoCount"),
    likes: stat("heartCount").or_else(|| stat("heart")),
    ..UserStats::default()
  };
  if let Some(n) = stat("friendCount") {
    stats.other.insert("friends".into(), n);
  }
  if let Some(n) = stat("diggCount") {
    stats.other.insert("liked".into(), n);
  }
  let mut out = user_brief(u);
  out.stats = stats;
  out.created_at = u.i64("createTime").filter(|t| *t > 0).and_then(from_secs);
  // relation: 0 none, 1 following, 2 friends (mutual).
  out.followed = u.i64("relation").map(|r| r == 1 || r == 2);
  for (key, path) in [
    ("language", "language"),
    ("bio_link", "bioLink.link"),
    ("commerce", "commerceUserInfo.commerceUser"),
    ("seller", "ttSeller"),
  ] {
    if let Some(v) = Some(u.at(path)).filter(|v| !v.is_null()) {
      out.extra.insert(key.into(), v.clone());
    }
  }
  out.raw = Some(info.clone());
  out
}

/// A user of a follower list or a user search (`user` + `stats`, either case).
pub fn listed_user(v: &Value) -> User {
  let info = if v.at("user_info").is_object() {
    v.at("user_info")
  } else {
    v
  };
  if info.at("user").is_object() {
    return user(info);
  }
  let mut out = user_brief(info);
  out.stats = UserStats {
    followers: info.count("follower_count"),
    following: info.count("following_count"),
    posts: info.count("aweme_count"),
    likes: info.count("total_favorited"),
    ..UserStats::default()
  };
  out
}

pub fn comment(v: &Value) -> Comment {
  let mut extra = social_core::Extra::new();
  if let Some(true) = v.bool("is_author_digged") {
    extra.insert("liked_by_author".into(), true.into());
  }
  if v.i64("stick_position").is_some_and(|n| n > 0) {
    extra.insert("pinned".into(), true.into());
  }
  if let Some(id) = v.str("aweme_id") {
    extra.insert("post_id".into(), id.into());
  }
  if let Some(label) = v.str("label_text") {
    extra.insert("label".into(), label.into());
  }
  let images: Vec<Value> = v
    .list("image_list")
    .iter()
    .filter_map(|i| i.str("origin_url.url_list.0").map(Value::from))
    .collect();
  if !images.is_empty() {
    extra.insert("images".into(), images.into());
  }
  Comment {
    id: v.str("cid").unwrap_or_default(),
    author: Some(user_brief(v.at("user"))).filter(|u| !u.id.is_empty() || !u.name.is_empty()),
    text: v.str("text").unwrap_or_default(),
    created_at: v.i64("create_time").and_then(from_secs),
    likes: v.count("digg_count"),
    reply_count: v.count("reply_comment_total"),
    reply_to: v.str("reply_to_username").filter(|s| !s.is_empty()),
    location: None,
    replies: Vec::new(),
    extra,
    raw: Some(v.clone()),
  }
}

/// A hashtag: `challengeInfo` (`challenge` + `stats`) or a bare challenge.
pub fn hashtag(info: &Value) -> Option<Collection> {
  let c = if info.at("challenge").is_object() {
    info.at("challenge")
  } else {
    info
  };
  let title = c.first_str(&["title", "cha_name"])?;
  let stat = |key: &str| {
    info
      .count(&format!("statsV2.{key}"))
      .or_else(|| info.count(&format!("stats.{key}")))
      .or_else(|| c.count(&format!("stats.{key}")))
  };
  Some(Collection {
    id: c.first_str(&["id", "cid"]).unwrap_or_default(),
    kind: "hashtag".into(),
    url: Some(format!(
      "{}/tag/{}",
      crate::api::WWW,
      crate::api::encode(&title)
    )),
    description: c.first_str(&["desc"]).filter(|d| !d.is_empty()),
    items: stat("videoCount"),
    views: stat("viewCount"),
    name: title,
    raw: Some(info.clone()),
    ..Collection::default()
  })
}

/// A sound: `musicInfo` (`music` + `stats`) or a bare music object.
pub fn sound(info: &Value) -> Option<Collection> {
  let m = if info.at("music").is_object() {
    info.at("music")
  } else {
    info
  };
  let id = m.str("id")?;
  let title = m.str("title").unwrap_or_default();
  let mut extra = social_core::Extra::new();
  for (key, path) in [
    ("author", "authorName"),
    ("original", "original"),
    ("duration", "duration"),
    ("play_url", "playUrl"),
  ] {
    let v = m.at(path);
    if !v.is_null() {
      extra.insert(key.into(), v.clone());
    }
  }
  Some(Collection {
    url: Some(format!("{}/music/{}-{id}", crate::api::WWW, slug(&title))),
    kind: "sound".into(),
    name: title,
    items: info
      .count("stats.videoCount")
      .or_else(|| m.count("userCount")),
    extra,
    raw: Some(info.clone()),
    id,
    ..Collection::default()
  })
}

/// A playlist (mix) or a favorites collection.
pub fn folder(v: &Value, kind: &str) -> Option<Collection> {
  let v = if v.at("mixInfo").is_object() {
    v.at("mixInfo")
  } else {
    v
  };
  let id = v.first_str(&["id", "mixId", "collectionId"])?;
  let name = v
    .first_str(&["name", "mixName", "collectionName"])
    .unwrap_or_default();
  let owner = v.at("creator");
  Some(Collection {
    url: owner
      .first_str(&["uniqueId", "unique_id"])
      .map(|h| format!("{}/@{h}/{kind}/{}-{id}", crate::api::WWW, slug(&name))),
    kind: kind.into(),
    items: v.count("videoCount").or_else(|| v.count("total")),
    owner: (!owner.is_null()).then(|| user_brief(owner)),
    name,
    raw: Some(v.clone()),
    id,
    ..Collection::default()
  })
}

fn slug(title: &str) -> String {
  let s: String = title
    .chars()
    .map(|c| if c.is_alphanumeric() { c } else { '-' })
    .collect();
  let s = s.trim_matches('-');
  if s.is_empty() { "x".into() } else { s.into() }
}
