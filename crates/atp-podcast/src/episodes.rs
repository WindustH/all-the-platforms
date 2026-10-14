//! One episode, read from its show's feed (with the whole show notes) and
//! found through Apple when given an Apple Podcasts link; and saving the
//! episode's file under the name the publisher's server gives it,
//! continuing a partial download.

use std::path::{Path, PathBuf};

use atp_core::download::{self, Link};
use atp_core::http::TOOL_USER_AGENT;
use atp_core::output::note;
use atp_core::text::decode_uri_component;
use atp_core::{Error, FileEntry, Media, MediaKind, Page, Post, Result, ValueExt, feed};

use crate::api::Api;
use crate::refs::{self, Episode, Item};
use crate::{parse, shows};

/// Apple lists at most this many of a show's newest episodes.
const APPLE_EPISODES: usize = 200;
/// File names too common to tell episodes apart (`media.mp3`): such files
/// are named after the episode instead.
const GENERIC: &[&str] = &[
  "audio", "content", "default", "download", "episode", "file", "index", "listen", "media", "play",
  "podcast", "redirect", "stream", "track",
];

/// A show or an episode.
pub async fn read_any(api: &Api, arg: &str) -> Result<Post> {
  match refs::item(arg)? {
    Item::Show(show) => shows::read(api, &show).await,
    Item::Episode(episode) => read(api, &episode).await,
  }
}

/// An episode and its file, for `download`.
pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let Item::Episode(episode) = refs::item(arg)? else {
    return Err(Error::input(
      "give an episode (from `episodes` or `search --filter episodes`), not a show",
    ));
  };
  let post = read(api, &episode).await?;
  let files = files(&post)?;
  Ok((post, files))
}

pub async fn read(api: &Api, episode: &Episode) -> Result<Post> {
  match episode {
    Episode::Feed { feed, guid } => from_feed(api, feed, guid).await,
    Episode::Apple { show, id } => from_apple(api, *show, *id).await,
  }
}

async fn from_feed(api: &Api, feed_url: &str, guid: &str) -> Result<Post> {
  let feed = feed::load(&api.ctx, feed_url).await?;
  let entry = feed.entry(guid).ok_or_else(|| {
    Error::not_found(format!("the feed {feed_url} has no episode {guid}"))
      .with_hint("the feed may no longer carry it; `atp podcast episodes FEED` lists what it holds")
  })?;
  Ok(parse::feed_episode(&feed, entry))
}

/// An Apple episode link: Apple's listing finds the feed and the episode's
/// GUID, the feed gives the episode; Apple's own data when the feed fails.
async fn from_apple(api: &Api, show: u64, id: u64) -> Result<Post> {
  let params = vec![
    ("id", show.to_string()),
    ("entity", "podcastEpisode".to_owned()),
    ("limit", APPLE_EPISODES.to_string()),
  ];
  let v = api.itunes("lookup", params).await?;
  let rows = v.list("results");
  let row = rows
    .iter()
    .find(|row| row.u64("trackId") == Some(id))
    .ok_or_else(|| {
      Error::not_found(format!(
        "Apple lists no episode {id} among the {APPLE_EPISODES} newest of show {show}"
      ))
    })?;
  let feed_url = row
    .str("feedUrl")
    .or_else(|| rows.iter().find_map(|r| r.str("feedUrl")));
  let listed = parse::apple_episode(row);
  if let (Some(feed_url), Some(guid)) = (feed_url, row.str("episodeGuid"))
    && let Ok(mut post) = from_feed(api, &feed_url, &guid).await
  {
    if let Some(apple) = listed.extra.get("apple_url") {
      post.extra.insert("apple_url".into(), apple.clone());
    }
    return Ok(post);
  }
  Ok(listed)
}

/// The episode's files: the enclosure the feed gives.
fn files(post: &Post) -> Result<Vec<Media>> {
  let media: Vec<Media> = post
    .media
    .iter()
    .filter(|m| {
      matches!(
        m.kind,
        MediaKind::Audio | MediaKind::Video | MediaKind::File
      )
    })
    .cloned()
    .collect();
  if media.is_empty() {
    return Err(Error::not_found(
      "this episode has no audio file in its feed",
    ));
  }
  Ok(media)
}

/// Save `media` in `dir` (`None`: a video wanted as audio only, which the
/// shared downloader extracts).
pub async fn save(
  api: &Api,
  media: &Media,
  audio_only: bool,
  dir: &Path,
  fallback: &str,
) -> Option<Result<PathBuf>> {
  if audio_only && media.kind == MediaKind::Video {
    return None;
  }
  Some(save_file(api, media, dir, fallback).await)
}

/// Save `media` in `dir` under the server's file name (or `fallback` plus
/// the extension, when that name is a generic one), through a `.part`
/// file a later run continues. A file already there is kept.
async fn save_file(api: &Api, media: &Media, dir: &Path, fallback: &str) -> Result<PathBuf> {
  let name = file_name(&media.url, media.kind, fallback);
  let out = dir.join(&name);
  if out.is_file() {
    note(&format!(
      "{} is already there (delete it to download again)",
      out.display()
    ));
    return Ok(out);
  }
  let entry = FileEntry {
    id: media.url.clone(),
    name,
    ..FileEntry::default()
  };
  let link = Link {
    url: media.url.clone(),
    headers: vec![("user-agent".into(), TOOL_USER_AGENT.into())],
  };
  let saved = download::files(
    &api.ctx,
    &entry,
    dir,
    async |_, _| Ok(Page::last(Vec::new())),
    async |_, _| Ok(link.clone()),
  )
  .await?;
  Ok(saved.first().map_or(out, |file| PathBuf::from(&file.path)))
}

/// The last part of the file's URL, decoded and made safe; a generic or
/// missing one gives way to `fallback` with the URL's extension.
fn file_name(url: &str, kind: MediaKind, fallback: &str) -> String {
  let path = url.split(['?', '#']).next().unwrap_or(url);
  let last = decode_uri_component(path.rsplit('/').next().unwrap_or_default());
  let last = safe(&last);
  let (stem, ext) = last.rsplit_once('.').unwrap_or((&last, ""));
  let ext = match (ext, kind) {
    (ext, _) if !ext.is_empty() && ext.len() <= 5 => ext,
    (_, MediaKind::Video) => "mp4",
    _ => "mp3",
  };
  let digits = stem.len() < 6 && stem.bytes().all(|b| b.is_ascii_digit());
  let generic = digits || stem.len() < 3 || GENERIC.contains(&stem.to_ascii_lowercase().as_str());
  if generic {
    format!("{}.{ext}", safe(fallback))
  } else {
    format!("{stem}.{ext}")
  }
}

/// A file name without separators or characters Windows refuses.
fn safe(name: &str) -> String {
  let cleaned: String = name
    .chars()
    .map(|c| {
      if c.is_control() || r#"/\:*?"<>|"#.contains(c) {
        '_'
      } else {
        c
      }
    })
    .collect();
  cleaned.trim().trim_end_matches(['.', ' ']).to_owned()
}
