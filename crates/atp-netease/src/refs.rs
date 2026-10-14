//! Parse what users pass in: songs, albums, playlists, artists, podcast
//! episodes and shows (ids, `kind:ID`, web and app links, `163cn.tv`
//! short links), users and comment threads.

use std::sync::LazyLock;

use atp_core::{Error, Result};
use regex::Regex;

use crate::api::{Api, WEB};

/// `song?id=1`, `/#/album?id=2`, `/m/playlist?id=3&…`, `/song/4/`.
static LINK: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(
    r"(?i)/(song|album|playlist|artist|program|dj|djradio|radio)(?:\?(?:[^#]*&)?id=|/)(\d+)",
  )
  .expect("valid regex")
});
static KIND_ID: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(?i)^([a-z]+):(\d+)$").expect("valid regex"));
static USER: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?i)/user(?:/home)?(?:\?(?:[^#]*&)?id=|/)(\d+)").expect("valid regex")
});

/// What a post argument names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostRef {
  Song(u64),
  Album(u64),
  Playlist(u64),
  Artist(u64),
  /// A podcast episode (节目, `program`).
  Episode(u64),
  /// A podcast show (播客 / 电台, `djradio`).
  Podcast(u64),
}

impl PostRef {
  pub fn id(self) -> u64 {
    match self {
      PostRef::Song(id)
      | PostRef::Album(id)
      | PostRef::Playlist(id)
      | PostRef::Artist(id)
      | PostRef::Episode(id)
      | PostRef::Podcast(id) => id,
    }
  }

  /// The comment thread of the item (songs, albums, playlists, episodes).
  pub fn thread(self) -> Result<String> {
    let prefix = match self {
      PostRef::Song(_) => "R_SO_4_",
      PostRef::Album(_) => "R_AL_3_",
      PostRef::Playlist(_) => "A_PL_0_",
      PostRef::Episode(_) => "A_DJ_1_",
      PostRef::Artist(_) | PostRef::Podcast(_) => {
        return Err(Error::input(
          "comments belong to songs, albums, playlists and podcast episodes",
        ));
      }
    };
    Ok(format!("{prefix}{}", self.id()))
  }
}

/// A post argument: a link, `song:ID` (also `album:`, `playlist:`,
/// `artist:`, `episode:`, `podcast:`), or a bare song id. Short links are
/// followed first.
pub async fn post(api: &Api, arg: &str) -> Result<PostRef> {
  let arg = arg.trim();
  if let Some(found) = parse_post(arg) {
    return Ok(found);
  }
  if is_short_link(arg) {
    let target = api.ctx.http.final_url(arg).await?;
    if let Some(found) = parse_post(&target) {
      return Ok(found);
    }
  }
  Err(Error::input(format!(
    "not a NetEase Cloud Music item: {arg} (a song id, a music.163.com link or kind:ID)"
  )))
}

fn parse_post(arg: &str) -> Option<PostRef> {
  if let Ok(id) = arg.parse() {
    return Some(PostRef::Song(id));
  }
  let (kind, id) = match KIND_ID.captures(arg) {
    Some(c) => (c[1].to_ascii_lowercase(), c[2].to_owned()),
    None => {
      let c = LINK.captures(arg)?;
      (c[1].to_ascii_lowercase(), c[2].to_owned())
    }
  };
  let id = id.parse().ok()?;
  Some(match kind.as_str() {
    "song" => PostRef::Song(id),
    "album" => PostRef::Album(id),
    "playlist" => PostRef::Playlist(id),
    "artist" => PostRef::Artist(id),
    "episode" | "program" | "dj" => PostRef::Episode(id),
    "podcast" | "djradio" | "radio" => PostRef::Podcast(id),
    _ => return None,
  })
}

fn is_short_link(arg: &str) -> bool {
  arg.starts_with("http") && (arg.contains("163cn.tv") || arg.contains("163cn.link"))
}

/// A user id from a numeric id or a profile link; `None` for anything
/// else (a nickname).
pub fn user_id(arg: &str) -> Option<String> {
  let arg = arg.trim().trim_start_matches("user:");
  if !arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit()) {
    return Some(arg.to_owned());
  }
  USER.captures(arg).map(|c| c[1].to_owned())
}

/// A numeric id (comments, cloud files).
pub fn number(arg: &str, what: &str) -> Result<u64> {
  arg
    .trim()
    .parse()
    .map_err(|_| Error::input(format!("not a {what} id: {arg}")))
}

pub fn song_url(id: impl std::fmt::Display) -> String {
  format!("{WEB}/song?id={id}")
}

pub fn album_url(id: impl std::fmt::Display) -> String {
  format!("{WEB}/album?id={id}")
}

pub fn playlist_url(id: impl std::fmt::Display) -> String {
  format!("{WEB}/playlist?id={id}")
}

pub fn artist_url(id: impl std::fmt::Display) -> String {
  format!("{WEB}/artist?id={id}")
}

pub fn episode_url(id: impl std::fmt::Display) -> String {
  format!("{WEB}/program?id={id}")
}

pub fn podcast_url(id: impl std::fmt::Display) -> String {
  format!("{WEB}/djradio?id={id}")
}

pub fn user_url(id: impl std::fmt::Display) -> String {
  format!("{WEB}/user/home?id={id}")
}
