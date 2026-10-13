//! Items: videos and photo posts, with their counters and media.

use super::*;

pub fn item(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let author = v.at("author");
  let handle = author
    .first_str(&["uniqueId", "unique_id"])
    .unwrap_or_default();
  let photo = !v.at("imagePost").is_null();
  Some(Post {
    url: Some(video_url(&handle, &id, photo)),
    kind: if photo { "photo" } else { "video" }.into(),
    title: v.str("imagePost.title"),
    text: v.str("desc"),
    author: (!author.is_null()).then(|| user_brief(author)),
    created_at: v.i64("createTime").and_then(from_secs),
    metrics: item_metrics(v),
    media: media(v),
    tags: item_tags(v),
    extra: item_extra(v),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  })
}

fn item_metrics(v: &Value) -> Metrics {
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
  metrics
}

/// Hashtags of the text and the challenges, each once (ignoring case).
fn item_tags(v: &Value) -> Vec<String> {
  let mut tags: Vec<String> = Vec::new();
  let names = v
    .list("textExtra")
    .iter()
    .filter_map(|t| t.str("hashtagName"))
    .chain(v.list("challenges").iter().filter_map(|c| c.str("title")));
  for name in names {
    if !tags.iter().any(|t| t.eq_ignore_ascii_case(&name)) {
      tags.push(name);
    }
  }
  tags
}

/// Music, duration, cover, places, ad / AI flags, your like / favorite.
fn item_extra(v: &Value) -> atp_core::Extra {
  let mut extra = atp_core::Extra::new();
  let mut put = |k: &str, value: Option<Value>| {
    if let Some(value) = value.filter(|x| !x.is_null()) {
      extra.insert(k.into(), value);
    }
  };
  let text = |path: &str| v.str(path).map(Value::from);
  let flag = |path: &str| v.bool(path).filter(|b| *b).map(Value::from);
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
  put("location_created", text("locationCreated"));
  put("poi", text("poi.name"));
  put("is_ad", flag("isAd"));
  put("ai_generated", flag("IsAigc"));
  put("liked", v.bool("digged").map(Value::from));
  put("favorited", v.bool("collected").map(Value::from));
  put("pinned", flag("isPinnedItem"));
  put("sec_uid", v.str("author.secUid").map(Value::from));
  extra
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
