//! Parse what users pass in: songs, albums, playlists, singers and charts
//! (mids, ids, `kind:ID`, y.qq.com and share links, short links), users
//! and comments; and the links ATP prints for them.

use std::sync::LazyLock;

use atp_core::{Error, Result};
use regex::Regex;

use crate::api::{Api, WEB};

/// Pages of the web player: `/n/ryqq_v2/songDetail/MID`,
/// `/n/yqq/album/MID.html`, `/playlist/ID` …
static PATH: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(
    r"(?i)/(songDetail|song|albumDetail|album|playlist|playsquare|singer|toplist)/([0-9A-Za-z]+)",
  )
  .expect("valid regex")
});
/// Share pages: `playsong.html?songmid=…`, `taoge.html?id=…` …
static QUERY: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?i)[?&](songmid|songid|albummid|albumid|singermid|disstid|topid)=([0-9A-Za-z]+)")
    .expect("valid regex")
});
static PAGE_ID: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?i)/(taoge|playlist|toplist)\.html\?(?:[^#]*&)?id=(\d+)").expect("valid regex")
});
static KIND_ID: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(?i)^([a-z]+):([0-9A-Za-z]+)$").expect("valid regex"));
static MID: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^[0-9A-Za-z]{14}$").expect("valid regex"));
static USER_LINK: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?i)[?&](?:uin|userid|hostuin|euin)=([^&#\s]+)").expect("valid regex")
});

/// A song or an album by its mid (`0039MnYb0qxYhV`) or its number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
  Mid(String),
  Id(u64),
}

impl Key {
  fn parse(text: &str) -> Key {
    match text.parse() {
      Ok(id) => Key::Id(id),
      Err(_) => Key::Mid(text.to_owned()),
    }
  }
}

/// What a post argument names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostRef {
  Song(Key),
  Album(Key),
  /// A playlist (歌单) by its id (`disstid`).
  Playlist(u64),
  /// A singer (歌手) by mid.
  Singer(String),
  /// A chart (榜单) by its `topId`.
  Toplist(u64),
}

/// A post argument: a link, `song:MID` (also `album:`, `playlist:`,
/// `singer:`, `toplist:`), or a bare song mid or number. Short links are
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
    "not a QQ Music item: {arg} (a song mid or id, a y.qq.com link or kind:ID)"
  )))
}

fn parse_post(arg: &str) -> Option<PostRef> {
  if arg.parse::<u64>().is_ok() || MID.is_match(arg) {
    return Some(PostRef::Song(Key::parse(arg)));
  }
  let (kind, id) = if let Some(c) = KIND_ID.captures(arg) {
    (c[1].to_ascii_lowercase(), c[2].to_owned())
  } else if let Some(c) = PATH.captures(arg) {
    (c[1].to_ascii_lowercase(), c[2].to_owned())
  } else if let Some(c) = PAGE_ID.captures(arg) {
    (c[1].to_ascii_lowercase(), c[2].to_owned())
  } else {
    let c = QUERY.captures(arg)?;
    (c[1].to_ascii_lowercase(), c[2].to_owned())
  };
  Some(match kind.as_str() {
    "song" | "songdetail" | "songmid" | "songid" => PostRef::Song(Key::parse(&id)),
    "album" | "albumdetail" | "albummid" | "albumid" => PostRef::Album(Key::parse(&id)),
    "playlist" | "playsquare" | "taoge" | "disstid" => PostRef::Playlist(id.parse().ok()?),
    "singer" | "artist" | "singermid" => PostRef::Singer(id),
    "toplist" | "chart" | "topid" => PostRef::Toplist(id.parse().ok()?),
    _ => return None,
  })
}

/// The singer mid of `singer:MID` or a singer link.
pub fn singer(arg: &str) -> Option<String> {
  match parse_post(arg.trim())? {
    PostRef::Singer(mid) => Some(mid),
    _ => None,
  }
}

/// Share links that redirect (`c6.y.qq.com/base/fcgi-bin/u?__=…`).
fn is_short_link(arg: &str) -> bool {
  arg.starts_with("http") && (arg.contains(".qq.com/") || arg.contains("url.cn/"))
}

/// What a user argument names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserRef {
  /// The encrypted account id QQ Music shows (`oK6FoK6q7e657n**`).
  Euin(String),
  /// A QQ Music account number.
  Uin(u64),
  /// `@name`: an exact nickname, found by search.
  Name(String),
}

/// A user argument: an encrypted id, an account number, a profile link
/// (`profile?uin=…`, `profile_v2/index.html?userid=…`) or `@name`.
pub fn user(arg: &str) -> Result<UserRef> {
  let arg = arg.trim().trim_start_matches("user:");
  if let Some(name) = arg.strip_prefix('@') {
    return Ok(UserRef::Name(name.trim().to_owned()));
  }
  let id = match USER_LINK.captures(arg) {
    Some(c) => c[1].replace("%2A", "*").replace("%2a", "*"),
    None if arg.starts_with("http") => {
      return Err(Error::input(format!("not a QQ Music profile link: {arg}")));
    }
    None => arg.to_owned(),
  };
  if id.is_empty() || id.contains(char::is_whitespace) {
    return Err(Error::input(format!(
      "not a QQ Music user: {arg} (an id, a profile link or @name)"
    )));
  }
  Ok(match id.parse() {
    Ok(n) => UserRef::Uin(n),
    Err(_) => UserRef::Euin(id),
  })
}

/// A comment id (`1!Yn6w…*`), as `comments` lists it.
pub fn comment(arg: &str) -> Result<String> {
  let id = arg.trim();
  if id.is_empty() || id.contains(char::is_whitespace) {
    return Err(Error::input(format!("not a comment id: {arg}")));
  }
  Ok(id.to_owned())
}

pub fn song_url(mid: &str) -> String {
  format!("{WEB}/n/ryqq_v2/songDetail/{mid}")
}

pub fn album_url(mid: &str) -> String {
  format!("{WEB}/n/ryqq_v2/albumDetail/{mid}")
}

pub fn playlist_url(id: &str) -> String {
  format!("{WEB}/n/ryqq_v2/playlist/{id}")
}

pub fn singer_url(mid: &str) -> String {
  format!("{WEB}/n/ryqq_v2/singer/{mid}")
}

pub fn toplist_url(id: &str) -> String {
  format!("{WEB}/n/ryqq_v2/toplist/{id}")
}

pub fn user_url(euin: &str) -> String {
  format!("{WEB}/n/ryqq_v2/profile?uin={euin}")
}

/// The cover of an album by its mid or `pmid` (`000MkMni19ClKG_5`).
pub fn album_cover(mid: &str) -> String {
  format!("https://y.gtimg.cn/music/photo_new/T002R500x500M000{mid}.jpg")
}

/// The picture of a singer by mid.
pub fn singer_picture(mid: &str) -> String {
  format!("https://y.gtimg.cn/music/photo_new/T001R500x500M000{mid}.jpg")
}
