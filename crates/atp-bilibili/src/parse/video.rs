//! Videos in their many shapes: `view`, popular / ranking / related lists,
//! search results, space uploads, favorites, history and watch-later items;
//! also the audio tracks and collections a favorites folder holds.

use atp_core::{Media, Metrics, Post, Value, ValueExt, json};

use super::{author, https, plain, secs};
use crate::refs::{Resource, Video, space_url};

/// Seconds from a number or a `m:ss` / `h:mm:ss` string.
pub fn duration(v: &Value, paths: &[&str]) -> Option<u64> {
  paths.iter().find_map(|p| match v.at(p) {
    Value::String(s) if s.contains(':') => s.split(':').try_fold(0u64, |acc, part| {
      Some(acc * 60 + part.trim().parse::<u64>().ok()?)
    }),
    _ => v.u64(p),
  })
}

/// Counters under the names each shape uses (`view.stat`, space `vlist`,
/// search results, favorites `cnt_info`).
fn metrics(v: &Value) -> Metrics {
  let mut m = Metrics {
    views: v.first_count(&["stat.view", "stat.play", "play", "cnt_info.play"]),
    likes: v.first_count(&["stat.like", "like", "cnt_info.thumb_up"]),
    comments: v.first_count(&["stat.reply", "comment", "review", "cnt_info.reply"]),
    shares: v.first_count(&["stat.share", "cnt_info.share"]),
    favorites: v.first_count(&["stat.favorite", "favorites", "cnt_info.collect"]),
    ..Metrics::default()
  };
  if let Some(n) = v.first_count(&["stat.coin", "cnt_info.coin"]) {
    m.other.insert("coins".into(), n);
  }
  let danmaku = [
    "stat.danmaku",
    "video_review",
    "danmaku",
    "cnt_info.danmaku",
  ];
  if let Some(n) = v.first_count(&danmaku) {
    m.other.insert("danmaku".into(), n);
  }
  m
}

/// A video from any listing or from `/x/web-interface/view`.
pub fn video(v: &Value) -> Post {
  let ids = v
    .first_str(&["bvid", "history.bvid"])
    .and_then(|b| Video::from_bvid(&b))
    .or_else(|| {
      v.first_str(&["aid", "history.oid", "id"])
        .and_then(|a| a.parse().ok())
        .map(Video::from_aid)
    });
  let mut p = Post {
    kind: "video".into(),
    title: plain(v, &["title"]),
    text: v
      .first_str(&["desc", "description", "intro"])
      .map(|s| s.trim().to_owned())
      .filter(|s| !s.is_empty() && s != "-"),
    author: author(
      v,
      &["owner.mid", "upper.mid", "author_mid", "mid"],
      &["owner.name", "upper.name", "author_name", "author"],
      &["owner.face", "upper.face", "author_face", "upic"],
    ),
    created_at: secs(v, &["pubdate", "created", "pubtime", "ctime"]),
    metrics: metrics(v),
    tags: v
      .str("tag")
      .map(|t| {
        t.split(',')
          .filter(|s| !s.is_empty())
          .map(String::from)
          .collect()
      })
      .unwrap_or_default(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(cover) = v.first_str(&["pic", "cover"]) {
    p.media.push(Media {
      alt: Some("cover".into()),
      ..Media::image(https(cover))
    });
  }
  if let Some(ids) = ids {
    p.id = ids.bvid.clone();
    p.url = Some(ids.url());
    p.extra.insert("bvid".into(), json!(ids.bvid));
    p.extra.insert("aid".into(), json!(ids.aid));
  }
  video_extra(v, &mut p);
  p
}

/// The video's cid, duration, category, ranking places and page count.
fn video_extra(v: &Value, p: &mut Post) {
  if let Some(cid) = ["cid", "history.cid"]
    .iter()
    .find_map(|path| v.u64(path))
    .filter(|c| *c > 0)
  {
    p.extra.insert("cid".into(), json!(cid));
  }
  if let Some(d) = duration(v, &["duration", "length", "duration_text"]) {
    p.extra.insert("duration".into(), json!(d));
  }
  if let Some(t) = v.first_str(&["tname", "typename", "tag_name"]) {
    p.extra.insert("tname".into(), json!(t));
  }
  // Best and current place on the site-wide ranking, once it made it there.
  for rank in ["his_rank", "now_rank"] {
    if let Some(r) = v.u64(&format!("stat.{rank}")).filter(|r| *r > 0) {
      p.extra.insert(rank.into(), json!(r));
    }
  }
  let pages = v.list("pages").len();
  if pages > 1 {
    p.extra.insert("pages".into(), json!(pages));
  }
}

/// An item of a favorites folder (`/x/v3/fav/resource/list`): a video
/// (type 2), an audio track (12) or a collection of videos (合集, 21),
/// with when it was saved; `None` for other kinds.
pub fn favorite(m: &Value) -> Option<Post> {
  let mut p = match m.u64("type")? {
    2 => video(m),
    12 => {
      let id = m.str("id")?;
      let mut p = held(m, "audio", format!("au{id}"), Resource::audio_url(&id));
      p.extra.insert("auid".into(), json!(id));
      p
    }
    21 => {
      let id = m.str("id")?;
      let url = m
        .str("upper.mid")
        .map(|mid| format!("{}/lists/{id}?type=season", space_url(mid)));
      let mut p = held(m, "season", format!("season:{id}"), url.unwrap_or_default());
      p.extra.insert("season_id".into(), json!(id));
      p
    }
    _ => return None,
  };
  if let Some(at) = m.i64("fav_time") {
    p.extra.insert("favorited_at".into(), json!(at));
  }
  Some(p)
}

/// A folder item that is not a video, under its own id and link.
fn held(m: &Value, kind: &str, id: String, url: String) -> Post {
  let mut p = Post {
    id,
    kind: kind.into(),
    title: plain(m, &["title"]),
    text: m
      .str("intro")
      .map(|s| s.trim().to_owned())
      .filter(|s| !s.is_empty()),
    url: Some(url).filter(|u| !u.is_empty()),
    author: author(m, &["upper.mid"], &["upper.name"], &["upper.face"]),
    created_at: secs(m, &["pubtime", "ctime"]),
    metrics: metrics(m),
    raw: Some(m.clone()),
    ..Post::default()
  };
  if let Some(cover) = m.str("cover") {
    p.media.push(Media {
      alt: Some("cover".into()),
      ..Media::image(https(cover))
    });
  }
  if let Some(d) = duration(m, &["duration"]) {
    p.extra.insert("duration".into(), json!(d));
  }
  p
}
