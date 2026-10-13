//! Dynamics (`/x/polymer/web-dynamic/v1/*`): text, image, video, forward, article ...

use social_core::{Media, Metrics, Post, Value, ValueExt, json};

use super::{https, secs, user, video};
use crate::refs::DYNAMIC_URL;

/// A dynamic as a post; forwards carry the original in `quoted`.
pub fn dynamic(v: &Value) -> Post {
  let modules = v.at("modules");
  let body = modules.at("module_dynamic");
  let major = body.at("major");
  let stat = modules.at("module_stat");
  let id = v.str("id_str").unwrap_or_default();
  let mut p = Post {
    url: (!id.is_empty()).then(|| format!("{DYNAMIC_URL}{id}")),
    id,
    kind: "dynamic".into(),
    title: major
      .first_str(&[
        "opus.title",
        "archive.title",
        "article.title",
        "pgc.title",
        "ugc_season.title",
        "common.title",
      ])
      .filter(|t| !t.is_empty()),
    text: body
      .first_str(&[
        "desc.text",
        "major.opus.summary.text",
        "major.archive.desc",
        "major.article.desc",
      ])
      .map(|t| t.trim().to_owned())
      .filter(|t| !t.is_empty()),
    author: Some(user(modules.at("module_author"))).filter(|u| !u.id.is_empty()),
    created_at: secs(modules, &["module_author.pub_ts"]),
    tags: body.str("topic.name").into_iter().collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  p.metrics = metrics(stat, major);
  p.media = media(major);
  if let Some(orig) = Some(v.at("orig")).filter(|o| o.str("id_str").is_some()) {
    p.quoted = Some(Box::new(dynamic(orig)));
  }
  if let Some(kind) = v.str("type") {
    let kind = kind
      .trim_start_matches("DYNAMIC_TYPE_")
      .to_ascii_lowercase();
    p.extra.insert("type".into(), json!(kind));
  }
  if let Some(bvid) = major.str("archive.bvid") {
    p.extra.insert("bvid".into(), json!(bvid));
  }
  if modules.str("module_tag.text").as_deref() == Some("置顶") {
    p.extra.insert("pinned".into(), json!(true));
  }
  p
}

/// Likes, comments and reposts; a video dynamic also carries the video's
/// plays and danmaku.
fn metrics(stat: &Value, major: &Value) -> Metrics {
  let mut metrics = Metrics {
    likes: stat.count("like.count"),
    comments: stat.count("comment.count"),
    shares: stat.count("forward.count"),
    views: major.count("archive.stat.play"),
    ..Metrics::default()
  };
  if let Some(n) = major.count("archive.stat.danmaku") {
    metrics.other.insert("danmaku".into(), n);
  }
  metrics
}

/// Images of an image / opus dynamic, then the cover of a video or article.
fn media(major: &Value) -> Vec<Media> {
  let images = major
    .list("draw.items")
    .iter()
    .map(|i| (i, "src"))
    .chain(major.list("opus.pics").iter().map(|i| (i, "url")));
  let mut media: Vec<Media> = images
    .filter_map(|(img, key)| {
      Some(Media {
        width: img.u64("width").map(|w| w as u32),
        height: img.u64("height").map(|h| h as u32),
        ..Media::image(https(img.str(key)?))
      })
    })
    .collect();
  if let Some(cover) = major.first_str(&["archive.cover", "article.covers.0"]) {
    media.push(Media {
      alt: Some("cover".into()),
      ..Media::image(https(cover))
    });
  }
  media
}

/// One entry of a dynamic's repost list (`detail/forward`): who reposted, with what text.
pub fn forward(v: &Value, of: &str) -> Post {
  let id = v.str("id_str").unwrap_or_default();
  let mut p = Post {
    url: (!id.is_empty()).then(|| format!("{DYNAMIC_URL}{id}")),
    id,
    kind: "dynamic".into(),
    text: v
      .first_str(&["desc.text", "desc"])
      .map(|t| t.trim().to_owned())
      .filter(|t| !t.is_empty()),
    author: Some(user(v.at("user"))).filter(|u| !u.id.is_empty()),
    created_at: secs(v, &["pub_ts", "ts"]),
    raw: Some(v.clone()),
    ..Post::default()
  };
  p.extra.insert("type".into(), json!("forward"));
  p.extra.insert("repost_of".into(), json!(of));
  if p.created_at.is_none()
    && let Some(when) = v.str("pub_time")
  {
    p.extra.insert("pub_time".into(), json!(when));
  }
  p
}

/// The video a video dynamic announces, with the dynamic's author, time and counters.
pub fn dynamic_video(v: &Value) -> Option<Post> {
  let modules = v.at("modules");
  let archive = modules.at("module_dynamic.major.archive");
  archive.str("bvid")?;
  let mut p = video(archive);
  p.author = Some(user(modules.at("module_author"))).filter(|u| !u.id.is_empty());
  p.created_at = secs(modules, &["module_author.pub_ts"]);
  p.metrics.likes = modules.count("module_stat.like.count");
  p.metrics.comments = modules.count("module_stat.comment.count");
  p.raw = Some(v.clone());
  Some(p)
}

/// One dynamic from the desktop detail endpoint (the one anonymous visitors
/// get reliably). It lists modules as an array under other names; this
/// rebuilds the web layout so [`dynamic`] and the comment lookup read both.
pub fn from_desktop(item: &Value) -> Value {
  let (mut author, mut desc, mut stat) = (Value::Null, Value::Null, Value::Null);
  let (mut major, mut orig) = (serde_json::Map::new(), Value::Null);
  for m in item.list("modules") {
    let user = m.at("module_author.user");
    if !user.is_null() {
      author = json!({
        "mid": user.at("mid"),
        "name": user.at("name"),
        "face": user.at("face"),
        "official_verify": {"type": user.at("official.type")},
        "pub_ts": m.at("module_author.pub_ts"),
      });
    }
    if let Some(text) = m.str("module_desc.text") {
      desc = json!({ "text": text });
    }
    for (k, v) in m.at("module_dynamic").as_object().into_iter().flatten() {
      let slot = match k.as_str() {
        "dyn_forward" => {
          orig = from_desktop(v.at("item"));
          continue;
        }
        "dyn_archive" => "archive",
        "dyn_draw" => "draw",
        "dyn_article" => "article",
        k if k.starts_with("dyn_") => "common",
        _ => continue,
      };
      major.insert(slot.into(), v.clone());
    }
    if m.at("module_stat").is_object() {
      stat = m.at("module_stat").clone();
    }
  }
  json!({
    "id_str": item.at("id_str"),
    "type": item.at("type"),
    "basic": {
      "comment_id_str": stat.at("comment.comment_id"),
      "comment_type": stat.at("comment.comment_type"),
    },
    "modules": {
      "module_author": author,
      "module_dynamic": {"desc": desc, "major": major},
      "module_stat": stat,
    },
    "orig": orig,
  })
}
