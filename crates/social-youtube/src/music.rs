//! YouTube Music (music.youtube.com, the `WEB_REMIX` client): search by
//! kind, songs of an album or artist page, and lyrics, as ytmusicapi and
//! YouTube.js `Music` read them.
//!
//! Every list item is a `musicResponsiveListItemRenderer`: its first flex
//! column is the title, the second the "Artist • Album • 3:45" byline whose
//! runs link artists (`UC…`) and albums (`MPREb…`).

use social_core::{Collection, Error, Post, Result, User, Value, ValueExt, json};

use crate::api::{Api, ClientType, MUSIC};
use crate::parse::{self, duration, text};

/// `music search --type`: the param ytmusicapi sends for each kind.
pub const KINDS: &[(&str, &str)] = &[
  ("songs", "EgWKAQIIAWoMEA4QChADEAQQCRAF"),
  ("videos", "EgWKAQIQAWoMEA4QChADEAQQCRAF"),
  ("albums", "EgWKAQIYAWoMEA4QChADEAQQCRAF"),
  ("artists", "EgWKAQIgAWoMEA4QChADEAQQCRAF"),
  ("playlists", "Eg-KAQwIABAAGAAgACgBMABqChAEEAMQCRAFEAo="),
  ("podcasts", "EgWKAQJQAWoMEA4QChADEAQQCRAF"),
];

fn song_url(id: &str) -> String {
  format!("{MUSIC}/watch?v={id}")
}

fn browse_url(id: &str) -> String {
  match id.strip_prefix("VL") {
    Some(list) => format!("{MUSIC}/playlist?list={list}"),
    None if id.starts_with("UC") => format!("{MUSIC}/channel/{id}"),
    None => format!("{MUSIC}/browse/{id}"),
  }
}

/// Runs of the n-th flex column.
fn column(r: &Value, n: usize) -> &[Value] {
  r.list(&format!(
    "flexColumns.{n}.musicResponsiveListItemFlexColumnRenderer.text.runs"
  ))
}

/// The page a run or item links to: `(browseId, page type)`.
fn link(v: &Value) -> Option<(String, String)> {
  let b = v.at("navigationEndpoint.browseEndpoint");
  let id = b.str("browseId")?;
  let kind = b
    .str("browseEndpointContextSupportedConfigs.browseEndpointContextMusicConfig.pageType")
    .unwrap_or_default();
  Some((id, kind))
}

/// Byline runs without separators.
fn byline(r: &Value) -> Vec<&Value> {
  column(r, 1)
    .iter()
    .filter(|run| run.str("text").is_some_and(|t| t.trim() != "•"))
    .collect()
}

fn thumbnail(r: &Value) -> Option<String> {
  parse::image(r.at("thumbnail.musicThumbnailRenderer.thumbnail"))
}

fn video_id(r: &Value) -> Option<String> {
  r.str("playlistItemData.videoId")
    .or_else(|| {
      column(r, 0)
        .first()
        .and_then(|run| run.str("navigationEndpoint.watchEndpoint.videoId"))
    })
    .or_else(|| parse::first(r.at("overlay"), "watchEndpoint").and_then(|w| w.str("videoId")))
}

/// A song or music video row.
fn song(r: &Value, owner: Option<&User>) -> Option<Post> {
  let id = video_id(r)?;
  let mut post = Post {
    url: Some(song_url(&id)),
    title: column(r, 0).first().and_then(|run| run.str("text")),
    raw: Some(r.clone()),
    ..Post::default()
  };
  let mut artists = byline_parts(&mut post, r);
  // Artist pages put the plays ("2.5B plays") in a later column.
  if post.metrics.views.is_none() {
    post.metrics.views = (2..4)
      .flat_map(|n| column(r, n))
      .filter_map(|run| run.str("text"))
      .find(|t| t.contains("play") || t.contains("view"))
      .and_then(|t| parse::count(&t));
  }
  // Album pages leave the artist out of each row.
  if artists.is_empty()
    && let Some(o) = owner
  {
    artists.push(o.clone());
  }
  if artists.len() > 1 {
    let names: Vec<Value> = artists.iter().map(|a| a.name.clone().into()).collect();
    post.extra.insert("artists".into(), names.into());
  }
  post.author = artists.into_iter().next();
  post.kind = post
    .extra
    .get("type")
    .and_then(|t| t.as_str())
    .unwrap_or("song")
    .to_owned();
  if let Some(t) = thumbnail(r) {
    post.media.push(social_core::Media {
      alt: Some("thumbnail".into()),
      ..social_core::Media::image(t)
    });
  }
  post.id = id;
  Some(post)
}

/// The byline of a song row: artists (returned), the album, the duration,
/// the plays and the type, put into `post`.
fn byline_parts(post: &mut Post, r: &Value) -> Vec<User> {
  let mut artists = Vec::new();
  for run in byline(r) {
    let t = run.str("text").unwrap_or_default();
    match link(run) {
      Some((bid, _)) if bid.starts_with("UC") => artists.push(User {
        url: Some(browse_url(&bid)),
        name: t,
        id: bid,
        ..User::default()
      }),
      Some((bid, kind)) if kind.ends_with("ALBUM") || bid.starts_with("MPRE") => {
        post
          .extra
          .insert("album".into(), json!({ "id": bid, "name": t }));
      }
      _ => byline_text(post, &mut artists, t),
    }
  }
  artists
}

/// An unlinked byline part: a duration, plays, the type, or an artist name.
fn byline_text(post: &mut Post, artists: &mut Vec<User>, text: String) {
  if let Some(secs) = duration(&text) {
    post.extra.insert("duration".into(), secs.into());
  } else if text.contains("view") || text.contains("play") {
    post.metrics.views = parse::count(&text);
  } else if matches!(text.as_str(), "Song" | "Video" | "Episode") {
    post.extra.insert("type".into(), text.to_lowercase().into());
  } else if artists.is_empty() {
    artists.push(User {
      id: text.clone(),
      name: text,
      ..User::default()
    });
  }
}

/// An album, single, playlist or artist row: its browse id, page type,
/// title and byline parts.
struct Entity {
  id: String,
  kind: String,
  title: String,
  parts: Vec<String>,
}

impl Entity {
  fn of(r: &Value) -> Option<Self> {
    let (id, kind) = link(r)?;
    let title = column(r, 0).first().and_then(|run| run.str("text"))?;
    let parts = byline(r).iter().filter_map(|run| run.str("text")).collect();
    Some(Self {
      id,
      kind,
      title,
      parts,
    })
  }

  fn artist(self) -> User {
    let mut user = User {
      url: Some(browse_url(&self.id)),
      bio: (!self.parts.is_empty()).then(|| self.parts.join(" • ")),
      id: self.id,
      name: self.title,
      ..User::default()
    };
    user.stats.followers = self
      .parts
      .iter()
      .find_map(|p| p.contains("subscriber").then(|| parse::count(p)).flatten());
    user
  }

  fn album(self) -> Collection {
    let kind = self
      .kind
      .strip_prefix("MUSIC_PAGE_TYPE_")
      .unwrap_or("playlist")
      .to_lowercase();
    Collection {
      url: Some(browse_url(&self.id)),
      description: (!self.parts.is_empty()).then(|| self.parts.join(" • ")),
      id: self.id,
      kind,
      name: self.title,
      ..Collection::default()
    }
  }
}

fn rows(v: &Value) -> Vec<&Value> {
  parse::find(v, "musicResponsiveListItemRenderer")
}

/// What `music search` found, by kind.
pub enum Found {
  Songs(Vec<Post>),
  Albums(Vec<Collection>),
  Artists(Vec<User>),
}

pub async fn search(api: &Api, query: &str, kind: &str) -> Result<Found> {
  let params = KINDS
    .iter()
    .find(|(k, _)| *k == kind)
    .map(|(_, p)| *p)
    .ok_or_else(|| Error::input(format!("unknown kind `{kind}`")))?;
  let v = api
    .call_as(
      ClientType::Music,
      "search",
      json!({ "query": query, "params": params }),
    )
    .await?;
  let rows = rows(&v);
  Ok(match kind {
    "songs" | "videos" | "podcasts" => {
      Found::Songs(rows.into_iter().filter_map(|r| song(r, None)).collect())
    }
    "artists" => Found::Artists(
      rows
        .into_iter()
        .filter_map(Entity::of)
        .map(Entity::artist)
        .collect(),
    ),
    _ => Found::Albums(
      rows
        .into_iter()
        .filter_map(Entity::of)
        .map(Entity::album)
        .collect(),
    ),
  })
}

/// Songs of an album (`MPREb…`), artist (`UC…`) or playlist page.
pub async fn tracks(api: &Api, id: &str) -> Result<Vec<Post>> {
  let id = id.trim();
  let browse_id = if id.starts_with("PL") || id.starts_with("OLAK") || id.starts_with("RD") {
    format!("VL{id}")
  } else {
    id.to_owned()
  };
  let v = api
    .call_as(
      ClientType::Music,
      "browse",
      json!({ "browseId": browse_id }),
    )
    .await?;
  let header = parse::first(&v, "musicResponsiveHeaderRenderer")
    .or_else(|| parse::first(&v, "musicImmersiveHeaderRenderer"))
    .or_else(|| parse::first(&v, "musicVisualHeaderRenderer"));
  // Album headers name the artist in the strapline; artist pages are the artist.
  let owner = header.and_then(|h| {
    let strap = h.list("straplineTextOne.runs").first();
    match strap.and_then(link) {
      Some((bid, _)) => Some(User {
        url: Some(browse_url(&bid)),
        name: strap.and_then(|s| s.str("text")).unwrap_or_default(),
        id: bid,
        ..User::default()
      }),
      None if browse_id.starts_with("UC") => Some(User {
        url: Some(browse_url(&browse_id)),
        name: text(h.at("title")).unwrap_or_default(),
        id: browse_id.clone(),
        ..User::default()
      }),
      None => None,
    }
  });
  let album = header.and_then(|h| text(h.at("title")));
  let mut posts: Vec<Post> = rows(&v)
    .into_iter()
    .filter_map(|r| song(r, owner.as_ref()))
    .collect();
  if browse_id.starts_with("MPRE") {
    for p in &mut posts {
      p.extra
        .entry("album".to_owned())
        .or_insert_with(|| json!({ "id": browse_id, "name": album }));
    }
  }
  if posts.is_empty() {
    return Err(Error::not_found(format!("no songs on {id}")));
  }
  Ok(posts)
}

/// Lyrics of a song: the watch panel's lyrics tab browses to them.
pub async fn lyrics(api: &Api, video: &str) -> Result<Value> {
  let next = api
    .call_as(
      ClientType::Music,
      "next",
      json!({ "videoId": video, "isAudioOnly": true }),
    )
    .await?;
  let browse_id = parse::find(&next, "browseEndpoint")
    .into_iter()
    .filter_map(|b| b.str("browseId"))
    .find(|id| id.starts_with("MPLYt"))
    .ok_or_else(|| Error::not_found(format!("no lyrics for {video}")))?;
  let v = api
    .call_as(
      ClientType::Music,
      "browse",
      json!({ "browseId": browse_id }),
    )
    .await?;
  let shelf = parse::first(&v, "musicDescriptionShelfRenderer")
    .ok_or_else(|| Error::not_found(format!("no lyrics for {video}")))?;
  Ok(json!({
    "video": video,
    "lyrics": text(shelf.at("description")),
    "source": text(shelf.at("footer")),
  }))
}
