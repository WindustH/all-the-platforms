//! Media of tweets and direct messages: photos at original size, videos and
//! GIFs as their best mp4 variant.

use atp_core::{Media, MediaKind, Value, ValueExt};

/// Photos at original size, videos and GIFs as their best mp4 variant.
pub(super) fn media(l: &Value) -> Vec<Media> {
  let items = match l.list("extended_entities.media") {
    [] => l.list("entities.media"),
    items => items,
  };
  items.iter().filter_map(media_item).collect()
}

fn media_item(m: &Value) -> Option<Media> {
  let thumb = m.str("media_url_https")?;
  let mut out = match m.str("type").as_deref() {
    Some("video") | Some("animated_gif") => {
      let best = m
        .list("video_info.variants")
        .iter()
        .filter(|v| v.str("content_type").as_deref() == Some("video/mp4"))
        .max_by_key(|v| v.u64("bitrate").unwrap_or(0))
        .and_then(|v| v.str("url"));
      // The variant's own size (`/vid/avc1/1080x1920/`); `original_info` is often scaled.
      let size = best.as_deref().and_then(variant_size);
      match best {
        Some(url) => {
          let kind = if m.str("type").as_deref() == Some("video") {
            MediaKind::Video
          } else {
            MediaKind::Gif
          };
          let mut media = Media::new(kind, url);
          media.duration = m.f64("video_info.duration_millis").map(|ms| ms / 1000.0);
          media.width = size.map(|(w, _)| w);
          media.height = size.map(|(_, h)| h);
          media
        }
        None => Media::image(thumb),
      }
    }
    _ => Media::image(original(&thumb)),
  };
  if out.width.is_none() {
    out.width = m.u64("original_info.width").map(|w| w as u32);
    out.height = m.u64("original_info.height").map(|h| h as u32);
  }
  out.alt = m.str("ext_alt_text");
  Some(out)
}

fn variant_size(url: &str) -> Option<(u32, u32)> {
  url.split('/').find_map(|seg| {
    let (w, h) = seg.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
  })
}

/// `…/media/abc.jpg` → `…/media/abc.jpg?name=orig`.
fn original(url: &str) -> String {
  if url.contains("pbs.twimg.com/media/") && !url.contains('?') {
    format!("{url}?name=orig")
  } else {
    url.to_owned()
  }
}

pub(super) fn attachment(a: &Value) -> Vec<Media> {
  let m = ["photo", "video", "animated_gif"]
    .iter()
    .map(|k| a.at(k))
    .find(|m| m.is_object());
  let Some(m) = m else {
    return Vec::new();
  };
  let best = m
    .list("video_info.variants")
    .iter()
    .filter(|v| v.str("content_type").as_deref() == Some("video/mp4"))
    .max_by_key(|v| v.u64("bitrate").unwrap_or(0))
    .and_then(|v| v.str("url"));
  match (best, m.str("media_url_https")) {
    (Some(url), _) => vec![Media::video(url)],
    (None, Some(url)) => vec![Media::image(url)],
    _ => Vec::new(),
  }
}
