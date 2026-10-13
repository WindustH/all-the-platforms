//! Videos: list renderers (`videoRenderer` and its grid / compact / playlist
//! variants, `lockupViewModel`, Shorts lockups) and the full watch page
//! (`player` details + `next` primary / secondary info).

use social_core::{Collection, Media, Post, User, Value, ValueExt};

use super::{byline, count, duration, find, first, image, owner, put, text, verified, when};
use crate::refs::{playlist_url, short_url, video_url};

pub(super) fn base(id: String, short: bool) -> Post {
  Post {
    kind: if short { "short" } else { "video" }.into(),
    url: Some(if short {
      short_url(&id)
    } else {
      video_url(&id)
    }),
    id,
    ..Post::default()
  }
}

pub(super) fn thumbnail(post: &mut Post, v: &Value) {
  if let Some(url) = image(v) {
    post.media.push(Media {
      alt: Some("thumbnail".into()),
      ..Media::image(url)
    });
  }
}

/// Published time, keeping YouTube's own wording next to the parsed value.
fn published(post: &mut Post, age: Option<String>) {
  if let Some(age) = age {
    post.created_at = when(&age);
    if age.starts_with("Streamed") {
      put(&mut post.extra, "stream", true);
    }
    put(&mut post.extra, "published", age);
  }
}

/// `videoRenderer`, `gridVideoRenderer`, `compactVideoRenderer`,
/// `playlistVideoRenderer`, `videoWithContextRenderer` and `reelItemRenderer`.
pub fn renderer(r: &Value, reel: bool) -> Option<Post> {
  let id = r.str("videoId")?;
  let styles: Vec<String> = find(r, "thumbnailOverlayTimeStatusRenderer")
    .iter()
    .filter_map(|o| o.str("style"))
    .collect();
  let short =
    reel || styles.iter().any(|s| s == "SHORTS") || first(r, "reelWatchEndpoint").is_some();
  let mut p = base(id, short);
  p.title = text(r.at("title")).or_else(|| text(r.at("headline")));
  p.text = text(r.at("descriptionSnippet")).or_else(|| {
    r.list("detailedMetadataSnippets")
      .first()
      .and_then(|s| text(s.at("snippetText")))
  });
  let author = ["ownerText", "longBylineText", "shortBylineText"]
    .iter()
    .find_map(|k| byline(r.at(k)));
  p.author = author.map(|mut a| {
    a.verified = verified(r.at("ownerBadges"));
    a
  });
  views_and_age(&mut p, r);
  renderer_extra(&mut p, r, &styles);
  thumbnail(&mut p, r.at("thumbnail"));
  p.raw = Some(r.clone());
  Some(p)
}

/// Views and age, from their own fields or from `videoInfo`, where playlist
/// entries put them: `1.2M views • 5 years ago`.
fn views_and_age(p: &mut Post, r: &Value) {
  let info: Vec<String> = r
    .list("videoInfo.runs")
    .iter()
    .filter_map(|x| x.str("text"))
    .filter(|t| t.trim() != "•")
    .collect();
  let views =
    text(r.at("viewCountText")).or_else(|| info.iter().find(|t| t.contains("view")).cloned());
  p.metrics.views = views.as_deref().and_then(count);
  let age =
    text(r.at("publishedTimeText")).or_else(|| info.iter().find(|t| t.contains("ago")).cloned());
  published(p, age);
}

/// Duration, live and upcoming states, playlist position and badges.
fn renderer_extra(p: &mut Post, r: &Value, styles: &[String]) {
  let length = text(r.at("lengthText"))
    .and_then(|t| duration(&t))
    .or_else(|| r.u64("lengthSeconds"));
  put(&mut p.extra, "duration", length);
  let live = styles.iter().any(|s| s == "LIVE")
    || r.list("badges").iter().any(|b| {
      b.str("metadataBadgeRenderer.style")
        .is_some_and(|s| s.contains("LIVE"))
    });
  if live {
    put(&mut p.extra, "live", true);
  }
  if let Some(start) = r.i64("upcomingEventData.startTime") {
    put(&mut p.extra, "upcoming", true);
    p.created_at = social_core::text::from_secs(start);
  }
  put(&mut p.extra, "position", r.u64("index.simpleText"));
  put(&mut p.extra, "set_video_id", r.str("setVideoId"));
  let badges: Vec<String> = r
    .list("badges")
    .iter()
    .filter_map(|b| b.str("metadataBadgeRenderer.label"))
    .collect();
  if !badges.is_empty() {
    put(&mut p.extra, "badges", badges);
  }
}

/// `shortsLockupViewModel` (Shorts shelves and the Shorts tab).
pub fn short(s: &Value) -> Option<Post> {
  let id = s
    .str("onTap.innertubeCommand.reelWatchEndpoint.videoId")
    .or_else(|| {
      s.str("entityId")
        .and_then(|e| e.strip_prefix("shorts-shelf-item-").map(str::to_owned))
        .filter(|id| id.len() == 11)
    })?;
  let mut p = base(id, true);
  p.title = text(s.at("overlayMetadata.primaryText"));
  p.metrics.views = text(s.at("overlayMetadata.secondaryText")).and_then(|t| count(&t));
  thumbnail(&mut p, s.at("thumbnailViewModel.thumbnailViewModel.image"));
  p.raw = Some(s.clone());
  Some(p)
}

/// What a `lockupViewModel` stands for.
pub enum Lockup {
  Video(Post),
  Playlist(Collection),
}

/// Texts of a lockup's metadata rows, with the channel link of a part if any.
fn parts(meta: &Value) -> Vec<(String, Option<&Value>)> {
  meta
    .list("metadata.contentMetadataViewModel.metadataRows")
    .iter()
    .flat_map(|r| r.list("metadataParts"))
    .filter_map(|p| {
      let t = p.str("text.content")?;
      let link = p
        .list("text.commandRuns")
        .iter()
        .map(|c| c.at("onTap.innertubeCommand"))
        .find(|c| {
          c.str("browseEndpoint.browseId")
            .is_some_and(|b| b.starts_with("UC"))
        });
      Some((t, link))
    })
    .collect()
}

/// The channel of a lockup: a linked metadata part or the avatar's link.
pub fn lockup_owner(l: &Value) -> Option<User> {
  let meta = l.at("metadata.lockupMetadataViewModel");
  parts(meta)
    .into_iter()
    .find_map(|(t, link)| owner(link?, Some(t)))
    .or_else(|| {
      let cmd = first(meta.at("image"), "innertubeCommand")?;
      owner(cmd, None)
    })
}

/// Texts of the thumbnail badges: a duration (`6:44`), `LIVE`, `9 lessons` ...
fn badges(l: &Value) -> Vec<String> {
  find(l.at("contentImage"), "thumbnailBadgeViewModel")
    .iter()
    .filter_map(|b| b.str("text"))
    .collect()
}

pub fn lockup(l: &Value) -> Option<Lockup> {
  let id = l.str("contentId")?;
  let kind = l.str("contentType").unwrap_or_default();
  let meta = l.at("metadata.lockupMetadataViewModel");
  let title = text(meta.at("title"));
  let parts = parts(meta);
  let badges = badges(l);
  if kind != "LOCKUP_CONTENT_TYPE_VIDEO" {
    return Some(Lockup::Playlist(lockup_playlist(
      l, id, &kind, title, &parts, &badges,
    )));
  }
  let mut p = base(id, false);
  p.title = title;
  p.author = lockup_owner(l);
  lockup_parts(&mut p, &parts);
  put(
    &mut p.extra,
    "duration",
    badges.iter().find_map(|b| duration(b)),
  );
  if badges.iter().any(|b| b == "LIVE") {
    put(&mut p.extra, "live", true);
  }
  thumbnail(&mut p, l.at("contentImage.thumbnailViewModel.image"));
  p.raw = Some(l.clone());
  Some(Lockup::Video(p))
}

/// A lockup of anything but a video: a playlist, mix, podcast ...
fn lockup_playlist(
  l: &Value,
  id: String,
  kind: &str,
  title: Option<String>,
  parts: &[(String, Option<&Value>)],
  badges: &[String],
) -> Collection {
  let kind = kind
    .strip_prefix("LOCKUP_CONTENT_TYPE_")
    .unwrap_or("playlist")
    .to_lowercase();
  let mut c = Collection {
    url: Some(playlist_url(&id)),
    name: title.unwrap_or_else(|| id.clone()),
    items: badges.iter().find_map(|b| count(b)),
    owner: lockup_owner(l),
    id,
    kind,
    raw: Some(l.clone()),
    ..Collection::default()
  };
  if let Some(updated) = parts.iter().find(|(t, _)| t.contains("pdated")) {
    put(&mut c.extra, "updated", updated.0.clone());
  }
  c
}

/// The metadata parts of a video lockup: the channel, the views (`1.2M
/// views`, or a bare `1M` in compact lists) and the age (`3 days ago`,
/// `4y ago`), each optional.
fn lockup_parts(p: &mut Post, parts: &[(String, Option<&Value>)]) {
  let mut name = None;
  for (t, _) in parts {
    let bare_count = !t.contains(' ') && t.starts_with(|c: char| c.is_ascii_digit());
    if t.ends_with("views") || t.ends_with("view") || t.ends_with("watching") || bare_count {
      p.metrics.views = count(t);
    } else if when(t).is_some() {
      if p.created_at.is_none() {
        published(p, Some(t.clone()));
      }
    } else if name.is_none() {
      name = Some(t.clone());
    }
  }
  if let (Some(a), Some(n)) = (p.author.as_mut(), name)
    && (a.name == a.id || a.handle.as_deref() == Some(a.name.trim_start_matches('@')))
  {
    a.name = n;
  }
}
