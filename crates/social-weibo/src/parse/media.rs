//! Pictures, live photos, GIFs and videos of a post.

use super::*;

pub(super) fn media(v: &Value) -> Vec<Media> {
  let mut out = Vec::new();
  let mix = v.list("mix_media_info.items");
  if !mix.is_empty() {
    for item in mix {
      match item.str("type").as_deref() {
        Some("video") => out.extend(video(item.at("data"))),
        _ => out.extend(picture(item.at("data"))),
      }
    }
    return out;
  }
  let infos = v.at("pic_infos");
  if infos.is_object() {
    let ids: Vec<String> = v
      .list("pic_ids")
      .iter()
      .filter_map(|p| p.as_str().map(str::to_owned))
      .collect();
    let order: Vec<String> = if ids.is_empty() {
      infos
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
    } else {
      ids
    };
    for id in order {
      out.extend(picture(infos.at(&id)));
    }
  } else {
    for p in v.list("pics") {
      out.extend(mobile_picture(p));
    }
  }
  if v.str("page_info.object_type").as_deref() == Some("video")
    || v.at("page_info.media_info").is_object()
  {
    out.extend(video(v.at("page_info")));
  }
  out
}

/// A web picture (`pic_infos` entry): the largest rendition; live photos and
/// GIFs also bring their motion part.
fn picture(item: &Value) -> Vec<Media> {
  let Some(url) = item.first_str(&["largest.url", "mw2000.url", "original.url", "large.url"])
  else {
    return Vec::new();
  };
  let kind = item.str("type").unwrap_or_default();
  let mut image = Media::image(url);
  image.width = item
    .first_str(&["largest.width", "original.width"])
    .and_then(|w| w.parse().ok());
  image.height = item
    .first_str(&["largest.height", "original.height"])
    .and_then(|h| h.parse().ok());
  let mut out = vec![image];
  if let Some(motion) = item.str("video").filter(|u| u.starts_with("http")) {
    let kind = if kind == "gif" {
      MediaKind::Gif
    } else {
      MediaKind::Video
    };
    out.push(Media::new(kind, motion));
  }
  out
}

fn mobile_picture(item: &Value) -> Vec<Media> {
  let Some(url) = item.first_str(&["large.url", "url"]) else {
    return Vec::new();
  };
  let mut out = vec![Media::image(url)];
  if let Some(motion) = item.str("videoSrc").filter(|u| u.starts_with("http")) {
    out.push(Media::video(motion));
  }
  out
}

/// A video `page_info`: the best stream of `playback_list`, else the best
/// named rendition.
fn video(info: &Value) -> Vec<Media> {
  let mi = info.at("media_info");
  let best = mi
    .list("playback_list")
    .iter()
    .filter(|e| e.str("play_info.url").is_some())
    .max_by_key(|e| {
      (
        e.i64("meta.quality_index").unwrap_or(0),
        e.i64("play_info.bitrate").unwrap_or(0),
      )
    });
  let mut media = match best {
    Some(e) => {
      let mut media = Media::video(e.str("play_info.url").unwrap_or_default());
      media.width = e.u64("play_info.width").map(|w| w as u32);
      media.height = e.u64("play_info.height").map(|h| h as u32);
      media.size = e.u64("play_info.size");
      media
    }
    None => {
      let Some(url) = mi
        .first_str(&[
          "mp4_1080p_mp4",
          "mp4_720p_mp4",
          "mp4_hd_url",
          "stream_url_hd",
          "mp4_sd_url",
          "stream_url",
        ])
        .or_else(|| {
          info.first_str(&[
            "urls.mp4_1080p_mp4",
            "urls.mp4_720p_mp4",
            "urls.mp4_hd_mp4",
            "urls.mp4_ld_mp4",
          ])
        })
      else {
        return Vec::new();
      };
      Media::video(url)
    }
  };
  media.duration = mi.f64("duration");
  media.alt = info.first_str(&["title", "page_title", "content1"]);
  vec![media]
}
