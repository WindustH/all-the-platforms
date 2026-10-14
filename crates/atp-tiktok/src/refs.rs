//! References: video / photo links and ids, short links (`vm.tiktok.com`,
//! `vt.tiktok.com`, `/t/`), `@handle`s and secUids, `#hashtags`, sounds,
//! playlists and live rooms.

use atp_core::{Error, Result};
use url::Url;

use crate::api::{Api, WWW};

/// A video or photo post: its id, and the author's handle when the link had it.
#[derive(Debug, Clone)]
pub struct PostRef {
  pub id: String,
  pub author: Option<String>,
  pub photo: bool,
}

impl PostRef {
  /// The page the post lives on (TikTok redirects a missing handle).
  pub fn url(&self) -> String {
    let kind = if self.photo { "photo" } else { "video" };
    format!(
      "{WWW}/@{}/{kind}/{}",
      self.author.as_deref().unwrap_or(""),
      self.id
    )
  }
}

/// A user: a handle (`tiktok`) or a secUid (`MS4wLjAB...`).
#[derive(Debug, Clone)]
pub enum UserRef {
  Handle(String),
  SecUid(String),
}

pub fn is_id(s: &str) -> bool {
  (6..=25).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

pub fn is_sec_uid(s: &str) -> bool {
  s.starts_with("MS4wLjAB") && s.len() > 30
}

fn parse_url(arg: &str) -> Option<Url> {
  let url = if arg.contains("://") {
    Url::parse(arg).ok()?
  } else if arg.contains(".com/") || arg.contains(".com?") {
    Url::parse(&format!("https://{arg}")).ok()?
  } else {
    return None;
  };
  url.host_str()?.ends_with("tiktok.com").then_some(url)
}

fn segments(url: &Url) -> Vec<String> {
  url
    .path_segments()
    .map(|s| s.filter(|p| !p.is_empty()).map(str::to_owned).collect())
    .unwrap_or_default()
}

/// A short link that only says where it leads after a redirect.
fn is_short(url: &Url) -> bool {
  let host = url.host_str().unwrap_or_default();
  host.starts_with("vm.")
    || host.starts_with("vt.")
    || segments(url).first().map(String::as_str) == Some("t")
}

/// Follow short links to the page they stand for.
async fn resolve(api: &Api, url: Url) -> Result<Url> {
  if !is_short(&url) {
    return Ok(url);
  }
  let target = api.ctx.http.final_url(url.as_str()).await?;
  Url::parse(&target).map_err(|_| Error::input(format!("{url} led nowhere usable")))
}

/// A post from an id, a link or a short link.
pub async fn post_ref(api: &Api, arg: &str) -> Result<PostRef> {
  let arg = arg.trim();
  if is_id(arg) {
    return Ok(PostRef {
      id: arg.to_owned(),
      author: None,
      photo: false,
    });
  }
  let bad = || {
    Error::input(format!(
      "not a TikTok video: {arg} (expected a video link, a short link or an id)"
    ))
  };
  let url = resolve(api, parse_url(arg).ok_or_else(bad)?).await?;
  post_from_url(&url).ok_or_else(bad)
}

/// `/@user/video/<id>`, `/@user/photo/<id>`, `/embed/v2/<id>`, `/v/<id>.html`.
pub fn post_from_url(url: &Url) -> Option<PostRef> {
  let segs = segments(url);
  let pos = segs
    .iter()
    .position(|s| matches!(s.as_str(), "video" | "photo" | "v2" | "v"))?;
  let id = segs.get(pos + 1)?.trim_end_matches(".html").to_owned();
  if !is_id(&id) {
    return None;
  }
  let author = segs
    .first()
    .and_then(|s| s.strip_prefix('@'))
    .filter(|h| !h.is_empty())
    .map(str::to_owned);
  Some(PostRef {
    id,
    author,
    photo: segs[pos] == "photo",
  })
}

/// A user from a handle, `@handle`, a profile link, a secUid or a short link.
pub async fn user_ref(api: &Api, arg: &str) -> Result<UserRef> {
  let arg = arg.trim();
  if is_sec_uid(arg) {
    return Ok(UserRef::SecUid(arg.to_owned()));
  }
  if let Some(url) = parse_url(arg) {
    let url = resolve(api, url).await?;
    let handle = segments(&url)
      .into_iter()
      .find_map(|s| s.strip_prefix('@').map(str::to_owned))
      .filter(|h| !h.is_empty());
    return handle
      .map(UserRef::Handle)
      .ok_or_else(|| Error::input(format!("no user in {arg}")));
  }
  let handle = arg.trim_start_matches('@');
  let valid = !handle.is_empty()
    && handle
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.');
  if valid {
    Ok(UserRef::Handle(handle.to_owned()))
  } else {
    Err(Error::input(format!(
      "not a TikTok user: {arg} (expected a handle, a profile link or a secUid)"
    )))
  }
}

/// A hashtag name from `#name`, `name` or a `/tag/<name>` link.
pub fn tag_name(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if let Some(url) = parse_url(arg) {
    let segs = segments(&url);
    if let Some(i) = segs.iter().position(|s| s == "tag")
      && let Some(name) = segs.get(i + 1)
    {
      return Ok(decode(name));
    }
    return Err(Error::input(format!("not a hashtag link: {arg}")));
  }
  let name = arg.trim_start_matches('#');
  if name.is_empty() {
    return Err(Error::input("empty hashtag"));
  }
  Ok(name.to_owned())
}

/// A sound id from an id or a `/music/<slug>-<id>` link.
pub fn music_id(arg: &str) -> Result<String> {
  trailing_id(arg, "music").ok_or_else(|| {
    Error::input(format!(
      "not a TikTok sound: {arg} (expected a sound link or id)"
    ))
  })
}

/// A playlist (mix) id from an id or a `/@user/playlist/<name>-<id>` link.
pub fn playlist_id(arg: &str) -> Result<String> {
  trailing_id(arg, "playlist").ok_or_else(|| {
    Error::input(format!(
      "not a TikTok playlist: {arg} (expected a playlist link or id)"
    ))
  })
}

/// A collection (favorites folder) id from an id or a `/@user/collection/<name>-<id>` link.
pub fn collection_id(arg: &str) -> Result<String> {
  trailing_id(arg, "collection").ok_or_else(|| {
    Error::input(format!(
      "not a TikTok collection: {arg} (expected a collection link or id)"
    ))
  })
}

/// The id after `<segment>/` in a link (`slug-<id>`), or `arg` itself when it is one.
fn trailing_id(arg: &str, segment: &str) -> Option<String> {
  let arg = arg.trim();
  if is_id(arg) {
    return Some(arg.to_owned());
  }
  let url = parse_url(arg)?;
  let segs = segments(&url);
  let i = segs.iter().position(|s| s == segment)?;
  let id = segs.get(i + 1)?.rsplit('-').next()?.to_owned();
  is_id(&id).then_some(id)
}

fn decode(s: &str) -> String {
  url::form_urlencoded::parse(format!("x={s}").as_bytes())
    .next()
    .map(|(_, v)| v.into_owned())
    .unwrap_or_else(|| s.to_owned())
}

pub fn video_url(author: &str, id: &str, photo: bool) -> String {
  let kind = if photo { "photo" } else { "video" };
  format!("{WWW}/@{author}/{kind}/{id}")
}

pub fn user_url(handle: &str) -> String {
  format!("{WWW}/@{handle}")
}

/// A live room: its host's handle, or the room id itself.
#[derive(Debug, Clone)]
pub enum LiveRef {
  Handle(String),
  Room(String),
}

/// A live room from `@handle`, a `/@handle/live` link or a room id, with
/// or without `live:` before it.
pub fn live(arg: &str) -> Result<LiveRef> {
  let arg = arg.trim();
  let arg = arg.strip_prefix("live:").unwrap_or(arg);
  let bad = || {
    Error::input(format!(
      "not a TikTok live room: {arg} (expected @handle, a tiktok.com/@handle/live link or a room id)"
    ))
  };
  if let Some(url) = parse_url(arg) {
    let segs = segments(&url);
    let handle = segs.first().and_then(|s| s.strip_prefix('@'));
    return handle
      .filter(|h| !h.is_empty())
      .map(|h| LiveRef::Handle(decode(h)))
      .ok_or_else(bad);
  }
  if arg.len() >= 15 && arg.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(LiveRef::Room(arg.to_owned()));
  }
  let handle = arg.trim_start_matches('@');
  if handle.is_empty() || handle.contains(['/', ' ', '?']) {
    return Err(bad());
  }
  Ok(LiveRef::Handle(handle.to_owned()))
}

pub fn live_url(handle: &str) -> String {
  format!("{WWW}/@{handle}/live")
}
