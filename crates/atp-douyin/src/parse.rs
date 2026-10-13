//! Douyin payloads → the shared model.
//!
//! An aweme is a video (`aweme_type` 0, 4, 61 ...) or an image post (68,
//! `images` set; images may carry a live-photo `video`). Counters live in
//! `statistics`, the author in `author`, hashtags in `text_extra`.

use atp_core::text::from_secs;
use atp_core::{
  Comment, Extra, Media, MediaKind, Metrics, Notification, Post, User, UserStats, Value, ValueExt,
  json,
};

use crate::refs::{note_url, user_url, video_url};

fn first_url(v: &Value, path: &str) -> Option<String> {
  v.str(&format!("{path}.url_list.0"))
}

fn put(extra: &mut Extra, key: &str, value: Option<Value>) {
  if let Some(v) = value.filter(|v| !v.is_null() && v != &json!("")) {
    extra.insert(key.into(), v);
  }
}

pub fn is_images(v: &Value) -> bool {
  !v.list("images").is_empty() || v.i64("aweme_type") == Some(68)
}

pub fn aweme(v: &Value) -> Option<Post> {
  let id = v.str("aweme_id")?;
  let images = is_images(v);
  let tags = v
    .list("text_extra")
    .iter()
    .filter_map(|t| t.str("hashtag_name"))
    .collect();
  let desc = v.first_str(&["desc", "caption"]);
  let title = v
    .first_str(&["item_title", "preview_title"])
    .filter(|t| Some(t) != desc.as_ref());
  Some(Post {
    url: Some(if images {
      note_url(&id)
    } else {
      video_url(&id)
    }),
    kind: if images { "note" } else { "video" }.into(),
    title,
    text: desc,
    author: v.at("author").as_object().and(user(v.at("author"))),
    created_at: v.i64("create_time").and_then(from_secs),
    metrics: aweme_metrics(v.at("statistics")),
    media: media(v),
    tags,
    extra: aweme_extra(v),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  })
}

fn aweme_metrics(stats: &Value) -> Metrics {
  let mut metrics = Metrics {
    views: stats.count("play_count").filter(|n| *n > 0),
    likes: stats.count("digg_count"),
    comments: stats.count("comment_count"),
    shares: stats.count("share_count"),
    favorites: stats.count("collect_count"),
    ..Metrics::default()
  };
  if let Some(n) = stats.count("recommend_count").filter(|n| *n > 0) {
    metrics.other.insert("recommends".into(), n);
  }
  if let Some(n) = stats.count("admire_count").filter(|n| *n > 0) {
    metrics.other.insert("admires".into(), n);
  }
  metrics
}

/// Type, location, pinned flag, mix, music, duration and share link.
fn aweme_extra(v: &Value) -> Extra {
  let mut extra = Extra::new();
  let text = |path: &str| v.str(path).map(Value::from);
  put(&mut extra, "aweme_type", v.at("aweme_type").clone().into());
  put(&mut extra, "location", text("ip_label"));
  put(&mut extra, "pinned", v.at("is_top").clone().into());
  put(&mut extra, "mix_id", text("mix_info.mix_id"));
  put(&mut extra, "mix", text("mix_info.mix_name"));
  put(&mut extra, "music", text("music.title"));
  put(&mut extra, "music_id", text("music.id_str"));
  put(
    &mut extra,
    "duration_ms",
    v.at("video.duration").clone().into(),
  );
  put(&mut extra, "share_url", text("share_url"));
  extra
}

/// Downloadable media: every image (and its live-photo clip) of an image
/// post, or the best video stream.
pub fn media(v: &Value) -> Vec<Media> {
  if is_images(v) {
    let mut out = Vec::new();
    for img in v.list("images") {
      let url = img
        .first_str(&["download_url_list.0", "url_list.0"])
        .unwrap_or_default();
      if url.is_empty() {
        continue;
      }
      let mut m = Media::image(url);
      m.width = img.u64("width").map(|n| n as u32);
      m.height = img.u64("height").map(|n| n as u32);
      out.push(m);
      if let Some(clip) = first_url(img, "video.play_addr") {
        out.push(Media::video(clip));
      }
    }
    return out;
  }
  best_video(v.at("video")).into_iter().collect()
}

/// The largest playable stream: the highest `bit_rate` entry, else
/// `play_addr` (both are watermark-free; `download_addr` carries the mark).
fn best_video(video: &Value) -> Option<Media> {
  let best = video
    .list("bit_rate")
    .iter()
    .filter(|b| first_url(b, "play_addr").is_some())
    .max_by_key(|b| {
      (
        b.u64("play_addr.height").unwrap_or(0),
        b.u64("bit_rate").unwrap_or(0),
      )
    });
  let (addr, size) = match best {
    Some(b) => (b.at("play_addr"), b.u64("play_addr.data_size")),
    None => (video.at("play_addr"), video.u64("play_addr.data_size")),
  };
  let url = addr.str("url_list.0")?;
  let mut m = Media::new(MediaKind::Video, url);
  m.width = addr
    .u64("width")
    .or_else(|| video.u64("width"))
    .map(|n| n as u32);
  m.height = addr
    .u64("height")
    .or_else(|| video.u64("height"))
    .map(|n| n as u32);
  m.duration = video.f64("duration").map(|ms| ms / 1000.0);
  m.size = size;
  Some(m)
}

/// The music of an aweme as an audio file.
pub fn music_media(v: &Value) -> Option<Media> {
  let url = first_url(v.at("music"), "play_url")?;
  let mut m = Media::new(MediaKind::Audio, url);
  m.duration = v.f64("music.duration");
  m.alt = v.str("music.title");
  Some(m)
}

pub fn user(v: &Value) -> Option<User> {
  let sec = v.str("sec_uid")?;
  let mut stats = UserStats {
    followers: v
      .count("follower_count")
      .or_else(|| v.count("mplatform_followers_count")),
    following: v.count("following_count"),
    posts: v.count("aweme_count"),
    likes: v.count("total_favorited"),
    ..UserStats::default()
  };
  if let Some(n) = v.count("favoriting_count") {
    stats.other.insert("liked".into(), n);
  }
  let mut extra = Extra::new();
  put(&mut extra, "uid", v.str("uid").map(Value::from));
  put(
    &mut extra,
    "short_id",
    v.str("short_id").filter(|s| s != "0").map(Value::from),
  );
  put(
    &mut extra,
    "verification",
    v.first_str(&["custom_verify", "enterprise_verify_reason"])
      .map(Value::from),
  );
  put(&mut extra, "gender", v.at("gender").clone().into());
  put(&mut extra, "school", v.str("school_name").map(Value::from));
  put(
    &mut extra,
    "live_room",
    v.str("room_id").filter(|r| r != "0").map(Value::from),
  );
  let followed = v.i64("follow_status").map(|s| s == 1 || s == 2);
  Some(User {
    id: sec.clone(),
    name: v.str("nickname").unwrap_or_default(),
    handle: v
      .first_str(&["unique_id", "short_id"])
      .filter(|h| !h.is_empty() && h != "0"),
    url: Some(user_url(&sec)),
    avatar: first_url(v, "avatar_larger")
      .or_else(|| first_url(v, "avatar_medium"))
      .or_else(|| first_url(v, "avatar_thumb")),
    bio: v.str("signature"),
    verified: v.str("custom_verify").is_some() || v.str("enterprise_verify_reason").is_some(),
    location: v
      .first_str(&["ip_location", "city", "province"])
      .map(|l| l.trim_start_matches("IP属地：").to_owned()),
    stats,
    followed,
    extra,
    raw: Some(v.clone()),
    ..User::default()
  })
}

pub fn comment(v: &Value) -> Option<Comment> {
  let id = v.str("cid")?;
  let pictures: Vec<Value> = v
    .list("image_list")
    .iter()
    .filter_map(|i| first_url(i, "origin_url").map(Value::from))
    .collect();
  let mut extra = Extra::new();
  put(
    &mut extra,
    "pictures",
    (!pictures.is_empty()).then(|| Value::from(pictures)),
  );
  put(&mut extra, "label", v.str("label_text").map(Value::from));
  put(
    &mut extra,
    "liked_by_author",
    v.at("is_author_digged").clone().into(),
  );
  put(
    &mut extra,
    "parent_id",
    v.str("reply_id").filter(|r| r != "0").map(Value::from),
  );
  put(
    &mut extra,
    "sticker",
    first_url(v, "sticker.static_url").map(Value::from),
  );
  Some(Comment {
    id,
    author: user(v.at("user")),
    text: v.str("text").unwrap_or_default(),
    created_at: v.i64("create_time").and_then(from_secs),
    likes: v.count("digg_count"),
    reply_count: v.count("reply_comment_total"),
    reply_to: v.str("reply_to_username"),
    location: v.str("ip_label"),
    extra,
    raw: Some(v.clone()),
    ..Comment::default()
  })
}

/// One entry of the hot board as a post: the word, its heat and the video
/// Douyin pins to it.
pub fn hot_word(v: &Value, rank: usize) -> Option<Post> {
  let word = v.str("word")?;
  let mut metrics = Metrics {
    views: v.count("view_count"),
    ..Metrics::default()
  };
  if let Some(h) = v.count("hot_value") {
    metrics.other.insert("heat".into(), h);
  }
  let mut extra = Extra::new();
  extra.insert(
    "rank".into(),
    json!(v.u64("position").unwrap_or(rank as u64)),
  );
  put(&mut extra, "label", v.at("label").clone().into());
  put(
    &mut extra,
    "sentence_id",
    v.str("sentence_id").map(Value::from),
  );
  put(&mut extra, "group_id", v.str("group_id").map(Value::from));
  let search = format!("{}/search/{}", crate::api::WWW, crate::sign::encode(&word));
  Some(Post {
    id: v
      .first_str(&["sentence_id", "group_id"])
      .unwrap_or_else(|| word.clone()),
    kind: "trend".into(),
    title: Some(word),
    url: Some(search),
    created_at: v.i64("event_time").and_then(from_secs),
    metrics,
    media: first_url(v, "word_cover")
      .map(Media::image)
      .into_iter()
      .collect(),
    extra,
    raw: Some(v.clone()),
    ..Post::default()
  })
}

/// One message of the notice center: the payload key it carries says what
/// happened (`digg`, `comment`, `follow`, `at` ...).
pub fn notice(v: &Value) -> Notification {
  let (kind, body) = [
    ("digg", "like"),
    ("comment", "comment"),
    ("follow", "follow"),
    ("at", "mention"),
    ("danmaku", "danmaku"),
  ]
  .iter()
  .find(|(key, _)| v.at(key).is_object())
  .map_or(("other", &Value::Null), |(key, kind)| (*kind, v.at(key)));
  let actor = ["from_user.0", "user", "from_user", "comment.user"]
    .iter()
    .find_map(|p| body.at(p).as_object().and(user(body.at(p))));
  let aweme = [body.at("aweme"), body.at("comment.aweme"), v.at("aweme")]
    .into_iter()
    .find(|a| a.is_object())
    .unwrap_or(&Value::Null);
  Notification {
    id: v.str("nid").unwrap_or_default(),
    kind: kind.into(),
    text: body
      .first_str(&["comment.text", "text", "content", "title"])
      .unwrap_or_default(),
    actor,
    target: aweme.first_str(&["desc", "preview_title"]),
    url: aweme.str("aweme_id").map(|id| video_url(&id)),
    created_at: v.i64("create_time").and_then(from_secs),
    unread: v.i64("has_read").map(|r| r == 0),
    raw: Some(v.clone()),
  }
}
