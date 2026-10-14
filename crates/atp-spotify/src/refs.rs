//! Parse what users pass in: Spotify URIs (`spotify:track:ID`),
//! open.spotify.com links (also with `intl-xx/`, `embed/` and `?si=`),
//! `kind:ID`, bare ids and spotify.link short links; and the links ATP
//! prints.

use atp_core::platform::either;
use atp_core::{Error, Result};

use crate::api::{Api, WEB};

/// What an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  Track,
  Album,
  Artist,
  Playlist,
  Show,
  Episode,
  User,
}

impl Kind {
  pub fn name(self) -> &'static str {
    match self {
      Kind::Track => "track",
      Kind::Album => "album",
      Kind::Artist => "artist",
      Kind::Playlist => "playlist",
      Kind::Show => "show",
      Kind::Episode => "episode",
      Kind::User => "user",
    }
  }

  /// `a track`, `an album`.
  pub fn a(self) -> String {
    let name = self.name();
    let article = if name.starts_with(['a', 'e']) {
      "an"
    } else {
      "a"
    };
    format!("{article} {name}")
  }

  fn from_name(name: &str) -> Option<Kind> {
    Some(match name.to_ascii_lowercase().as_str() {
      "track" | "song" => Kind::Track,
      "album" => Kind::Album,
      "artist" => Kind::Artist,
      "playlist" => Kind::Playlist,
      "show" | "podcast" => Kind::Show,
      "episode" => Kind::Episode,
      "user" => Kind::User,
      _ => return None,
    })
  }
}

/// One item by kind and id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
  pub kind: Kind,
  pub id: String,
}

impl Item {
  /// `spotify:track:ID`.
  pub fn uri(&self) -> String {
    format!("spotify:{}:{}", self.kind.name(), self.id)
  }

  pub fn url(&self) -> String {
    url(self.kind, &self.id)
  }
}

/// The open.spotify.com page of an item.
pub fn url(kind: Kind, id: &str) -> String {
  format!("{WEB}/{}/{id}", kind.name())
}

/// An item argument; a bare id is taken as a `bare`. Short links are
/// followed first.
pub async fn item(api: &Api, arg: &str, bare: Kind) -> Result<Item> {
  let arg = arg.trim();
  if let Some(found) = parse(arg, bare) {
    return Ok(found);
  }
  if arg.starts_with("https://spotify.link/") || arg.starts_with("https://spotify.app.link/") {
    let target = api.ctx.http.final_url(arg).await?;
    if let Some(found) = parse(&target, bare) {
      return Ok(found);
    }
  }
  Err(Error::input(format!(
    "not a Spotify item: {arg} (a link, a spotify: URI, kind:ID or an id)"
  )))
}

/// An item argument that must be one of `kinds`.
pub async fn expect(api: &Api, arg: &str, bare: Kind, kinds: &[Kind]) -> Result<Item> {
  let found = item(api, arg, bare).await?;
  if kinds.contains(&found.kind) {
    return Ok(found);
  }
  let wanted: Vec<String> = kinds.iter().map(|k| k.a()).collect();
  Err(Error::input(format!(
    "{arg} is {}; expected {}",
    found.kind.a(),
    either(&wanted)
  )))
}

/// `spotify:KIND:ID`, a link, `KIND:ID` or a bare id (a `bare`).
pub fn parse(arg: &str, bare: Kind) -> Option<Item> {
  let arg = arg.trim();
  if let Some(rest) = arg.strip_prefix("spotify:") {
    return kind_id(rest);
  }
  if arg.contains("spotify.com/") {
    return link(arg);
  }
  if arg.contains(':') {
    return kind_id(arg);
  }
  valid(bare, arg)
}

fn kind_id(text: &str) -> Option<Item> {
  let (kind, id) = text.split_once(':')?;
  valid(Kind::from_name(kind)?, id)
}

/// `https://open.spotify.com/intl-de/track/ID?si=…`, `/embed/album/ID`,
/// and the old `/user/NAME/playlist/ID`.
fn link(arg: &str) -> Option<Item> {
  let full = if arg.starts_with("http") {
    arg.to_owned()
  } else {
    format!("https://{arg}")
  };
  let url = url::Url::parse(&full).ok()?;
  if !matches!(url.host_str()?, "open.spotify.com" | "play.spotify.com") {
    return None;
  }
  let mut segments = url
    .path_segments()?
    .filter(|s| !s.is_empty() && !s.starts_with("intl-") && *s != "embed");
  let kind = Kind::from_name(segments.next()?)?;
  let id = segments.next()?;
  if kind == Kind::User && segments.next() == Some("playlist") {
    return valid(Kind::Playlist, segments.next()?);
  }
  valid(kind, id)
}

fn valid(kind: Kind, id: &str) -> Option<Item> {
  let id = id.trim();
  let ok = match kind {
    Kind::User => !id.is_empty() && !id.contains(|c: char| c.is_whitespace() || c == '/'),
    _ => is_catalog_id(id),
  };
  ok.then(|| Item {
    kind,
    id: id.to_owned(),
  })
}

/// Catalog ids are 22 base-62 characters.
fn is_catalog_id(id: &str) -> bool {
  id.len() == 22 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// A user argument: `None` for `me`, else the account id of a link, a
/// `spotify:user:` URI, `user:ID` or a bare id.
pub fn user(arg: &str) -> Result<Option<String>> {
  let arg = arg.trim();
  if arg.eq_ignore_ascii_case("me") {
    return Ok(None);
  }
  match parse(arg.trim_start_matches('@'), Kind::User) {
    Some(Item {
      kind: Kind::User,
      id,
    }) => Ok(Some(id)),
    Some(other) => Err(Error::input(format!(
      "{arg} is {}, not a user",
      other.kind.a()
    ))),
    None => Err(Error::input(format!(
      "not a Spotify user: {arg} (a profile link, spotify:user:ID, an id or me)"
    ))),
  }
}
