//! Posts in either shape (feed photos, carousels, reels, IGTV, stories),
//! their files, and the lists they arrive in.

use atp_core::text::from_secs;
use atp_core::{Media, MediaKind, Metrics, Page, Post, Value, ValueExt};

use super::owner;
use crate::api::WEB;
use crate::refs;

/// A post (feed photo, carousel, reel, IGTV) in either shape.
pub fn post(v: &Value) -> Post {
  let code = v.first_str(&["code", "shortcode"]);
  let id = v
    .first_str(&["pk", "id"])
    .map(|i| i.split('_').next().unwrap_or(&i).to_owned())
    .or_else(|| code.as_deref().and_then(refs::decode))
    .unwrap_or_default();
  let code = code.or_else(|| refs::encode(&id));
  let kind = kind(v);
  let text = v
    .str("caption.text")
    .or_else(|| v.str("edge_media_to_caption.edges.0.node.text"));
  let mut post = Post {
    id,
    kind: kind.into(),
    text,
    url: code.as_deref().map(|c| match kind {
      "reel" => format!("{WEB}/reel/{c}/"),
      _ => refs::post_url(c),
    }),
    author: owner(v),
    created_at: v
      .first_str(&["taken_at", "taken_at_timestamp"])
      .and_then(|t| t.parse().ok())
      .and_then(from_secs),
    metrics: metrics(v),
    media: files(v),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(code) = &code {
    post.extra.insert("shortcode".into(), code.clone().into());
  }
  if let Some(location) = v.str("location.name") {
    post.extra.insert("location".into(), location.into());
  }
  post
}

/// `reel`, `igtv`, `carousel`, `video` or `photo`.
fn kind(v: &Value) -> &'static str {
  let product = v.str("product_type").unwrap_or_default();
  let video = v.i64("media_type") == Some(2) || v.bool("is_video") == Some(true);
  match (product.as_str(), v.i64("media_type")) {
    ("clips", _) => "reel",
    ("igtv", _) => "igtv",
    (_, Some(8)) => "carousel",
    _ if v.at("edge_sidecar_to_children").is_object() => "carousel",
    _ if video => "video",
    _ => "photo",
  }
}

fn metrics(v: &Value) -> Metrics {
  Metrics {
    likes: v.first_count(&[
      "like_count",
      "edge_media_preview_like.count",
      "edge_liked_by.count",
    ]),
    comments: v.first_count(&[
      "comment_count",
      "edge_media_to_parent_comment.count",
      "edge_media_to_comment.count",
    ]),
    views: v.first_count(&[
      "play_count",
      "ig_play_count",
      "view_count",
      "video_play_count",
      "video_view_count",
    ]),
    shares: v.count("reshare_count"),
    ..Metrics::default()
  }
}

/// Every image / video of a post at its largest size (carousels flattened).
fn files(v: &Value) -> Vec<Media> {
  let children: Vec<&Value> = if !v.list("carousel_media").is_empty() {
    v.list("carousel_media").iter().collect()
  } else if v.at("edge_sidecar_to_children").is_object() {
    v.list("edge_sidecar_to_children.edges")
      .iter()
      .map(|e| e.at("node"))
      .collect()
  } else {
    vec![v]
  };
  children.into_iter().filter_map(file).collect()
}

fn file(v: &Value) -> Option<Media> {
  let video = v
    .list("video_versions")
    .iter()
    .max_by_key(|x| x.u64("width").unwrap_or(0) * x.u64("height").unwrap_or(0))
    .cloned()
    .or_else(|| {
      v.str("video_url")
        .map(|u| serde_json::json!({"url": u, "width": v.at("dimensions.width"), "height": v.at("dimensions.height")}))
    });
  if let Some(vid) = video.filter(|x| x.str("url").is_some()) {
    let mut m = Media::new(MediaKind::Video, vid.str("url")?);
    m.width = vid.u64("width").map(|w| w as u32);
    m.height = vid.u64("height").map(|h| h as u32);
    m.duration = v.f64("video_duration");
    return Some(m);
  }
  let best = v
    .list("image_versions2.candidates")
    .iter()
    .max_by_key(|x| x.u64("width").unwrap_or(0))
    .cloned();
  let (url, w, h) = match best {
    Some(b) => (b.str("url")?, b.u64("width"), b.u64("height")),
    None => (
      v.str("display_url")?,
      v.u64("dimensions.width"),
      v.u64("dimensions.height"),
    ),
  };
  let mut m = Media::image(url);
  m.width = w.map(|x| x as u32);
  m.height = h.map(|x| x as u32);
  m.alt = v.str("accessibility_caption");
  Some(m)
}

/// Items of a v1 feed (`items[]`, each a post or `{media}`) and its cursor.
pub fn items(v: &Value) -> Page<Post> {
  let posts = v
    .list("items")
    .iter()
    .map(|item| {
      post(if item.at("media").is_object() {
        item.at("media")
      } else {
        item
      })
    })
    .collect();
  let next = (v.bool("more_available") != Some(false))
    .then(|| v.str("next_max_id"))
    .flatten();
  Page::new(posts, next)
}

/// Grid sections (`layout_content.medias[].media`) and their `max_id|page` cursor.
pub fn sections(v: &Value) -> Page<Post> {
  let posts = v
    .list("sections")
    .iter()
    .flat_map(|section| {
      section
        .list("layout_content.medias")
        .iter()
        .chain(section.list("layout_content.fill_items"))
        .map(|m| post(m.at("media")))
        .collect::<Vec<_>>()
    })
    .collect();
  let next = (v.bool("more_available") == Some(true))
    .then(|| {
      v.str("next_max_id")
        .map(|m| format!("{m}|{}", v.str("next_page").unwrap_or_else(|| "1".into())))
    })
    .flatten();
  Page::new(posts, next)
}
