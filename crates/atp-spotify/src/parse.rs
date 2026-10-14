//! Web API objects -> core models: tracks, albums, artists, playlists,
//! shows and episodes as posts (id the Spotify id, `url` the
//! open.spotify.com page); accounts and followed artists as users.

use atp_core::text::{date_only, html_to_text};
use atp_core::{Collection, Media, Post, User, Value, ValueExt, json};

use crate::refs::{self, Kind};

/// Any item by its `type`: playlist entries, the player's current item,
/// search results.
pub fn item(v: &Value) -> Option<Post> {
  match v.str("type")?.as_str() {
    "track" => track(v),
    "episode" => episode(v),
    "album" => album(v),
    "artist" => artist(v),
    "playlist" => playlist(v),
    "show" => show(v),
    _ => None,
  }
}

/// The largest image (Spotify lists them widest first).
fn cover(v: &Value) -> Vec<Media> {
  let image = v.at("images.0");
  let Some(url) = image.str("url") else {
    return Vec::new();
  };
  let mut media = Media::image(url);
  media.width = image.u64("width").and_then(|w| u32::try_from(w).ok());
  media.height = image.u64("height").and_then(|h| u32::try_from(h).ok());
  vec![media]
}

/// `[{id, name}]` of a track's or an album's artists, and the author line:
/// their names joined, the first one's id.
fn artists(v: &Value) -> (Vec<Value>, Option<User>) {
  let list: Vec<Value> = v
    .list("artists")
    .iter()
    .filter_map(|a| Some(json!({"id": a.str("id"), "name": a.str("name")?})))
    .collect();
  let names: Vec<String> = list.iter().filter_map(|a| a.str("name")).collect();
  let author = list.first().map(|first| {
    let id = first.str("id").unwrap_or_default();
    User {
      url: (!id.is_empty()).then(|| refs::url(Kind::Artist, &id)),
      name: names.join(" / "),
      id,
      ..User::default()
    }
  });
  (list, author)
}

/// The release date when Spotify gives the day; a year or a month alone
/// stays in `extra.release_date`.
fn released(v: &Value) -> Option<jiff::Timestamp> {
  if v.str("release_date_precision").as_deref() != Some("day") {
    return None;
  }
  date_only(v.str("release_date")?.parse().ok()?)
}

/// Copy what is at `path` into `extra.key`, when there is something.
fn keep(p: &mut Post, v: &Value, pairs: &[(&str, &str)]) {
  for (key, path) in pairs {
    let value = v.at(path);
    if !value.is_null() && !p.extra.contains_key(*key) {
      p.extra.insert((*key).into(), value.clone());
    }
  }
}

/// `duration_ms` as whole seconds in `extra.duration`.
fn duration(p: &mut Post, v: &Value) {
  if let Some(ms) = v.u64("duration_ms").filter(|ms| *ms > 0) {
    p.extra.insert("duration".into(), json!(ms.div_ceil(1000)));
  }
}

/// Plain text of a description (playlists send HTML entities and links).
fn description(v: &Value) -> Option<String> {
  let text = html_to_text(&v.str("description")?);
  let text = text.trim();
  (!text.is_empty()).then(|| text.to_owned())
}

/// A track (`kind` `track`): artists as the author, the album's cover and
/// release date, the duration. Never any audio.
pub fn track(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let album = v.at("album");
  let (list, author) = artists(v);
  let mut p = Post {
    url: Some(refs::url(Kind::Track, &id)),
    kind: "track".into(),
    title: v.str("name"),
    author,
    created_at: released(album),
    media: cover(album),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  duration(&mut p, v);
  p.extra.insert("artists".into(), Value::Array(list));
  if let Some(name) = album.str("name") {
    p.extra
      .insert("album".into(), json!({"id": album.str("id"), "name": name}));
  }
  keep(&mut p, album, &[("release_date", "release_date")]);
  let pairs = [
    ("explicit", "explicit"),
    ("track_number", "track_number"),
    ("disc_number", "disc_number"),
    ("isrc", "external_ids.isrc"),
    ("uri", "uri"),
  ];
  keep(&mut p, v, &pairs);
  Some(p)
}

/// An album (`kind` `album`).
pub fn album(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let (list, author) = artists(v);
  let mut p = Post {
    url: Some(refs::url(Kind::Album, &id)),
    kind: "album".into(),
    title: v.str("name"),
    author,
    created_at: released(v),
    media: cover(v),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  p.extra.insert("artists".into(), Value::Array(list));
  let pairs = [
    ("type", "album_type"),
    ("track_count", "total_tracks"),
    ("release_date", "release_date"),
    ("upc", "external_ids.upc"),
    ("copyright", "copyrights.0.text"),
    ("uri", "uri"),
  ];
  keep(&mut p, v, &pairs);
  Some(p)
}

/// An artist (`kind` `artist`) as a post, for search and `read`.
pub fn artist(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let mut p = Post {
    url: Some(refs::url(Kind::Artist, &id)),
    kind: "artist".into(),
    title: v.str("name"),
    media: cover(v),
    tags: v.list("genres").iter().filter_map(|g| g.str("")).collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  p.metrics.favorites = v.count("followers.total");
  keep(&mut p, v, &[("uri", "uri")]);
  Some(p)
}

/// A followed artist as a user, for `following`.
pub fn artist_user(v: &Value) -> Option<User> {
  let id = v.str("id")?;
  let mut u = User {
    url: Some(refs::url(Kind::Artist, &id)),
    name: v.str("name").unwrap_or_default(),
    avatar: v.str("images.0.url"),
    followed: Some(true),
    raw: Some(v.clone()),
    id,
    ..User::default()
  };
  u.extra.insert("kind".into(), json!("artist"));
  Some(u)
}

/// A playlist (`kind` `playlist`); the count of its items whether they
/// come as `items` (2026) or `tracks`.
pub fn playlist(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let mut p = Post {
    url: Some(refs::url(Kind::Playlist, &id)),
    kind: "playlist".into(),
    title: v.str("name"),
    text: description(v),
    author: user(v.at("owner")).map(|u| User { raw: None, ..u }),
    media: cover(v),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  p.metrics.favorites = v.count("followers.total");
  if let Some(n) = v.first_count(&["items.total", "tracks.total"]) {
    p.extra.insert("track_count".into(), json!(n));
  }
  let pairs = [
    ("public", "public"),
    ("collaborative", "collaborative"),
    ("snapshot_id", "snapshot_id"),
    ("uri", "uri"),
  ];
  keep(&mut p, v, &pairs);
  Some(p)
}

/// A show (podcast, `kind` `show`).
pub fn show(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let mut p = Post {
    url: Some(refs::url(Kind::Show, &id)),
    kind: "show".into(),
    title: v.str("name"),
    text: description(v),
    media: cover(v),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  let pairs = [
    ("episode_count", "total_episodes"),
    ("explicit", "explicit"),
    ("languages", "languages"),
    ("media_type", "media_type"),
    ("uri", "uri"),
  ];
  keep(&mut p, v, &pairs);
  Some(p)
}

/// An episode (`kind` `episode`): its show as the author.
pub fn episode(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let show = v.at("show");
  let author = show.str("id").map(|show_id| User {
    url: Some(refs::url(Kind::Show, &show_id)),
    name: show.str("name").unwrap_or_default(),
    id: show_id,
    ..User::default()
  });
  let mut p = Post {
    url: Some(refs::url(Kind::Episode, &id)),
    kind: "episode".into(),
    title: v.str("name"),
    text: description(v),
    author,
    created_at: released(v),
    media: cover(v),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  duration(&mut p, v);
  let pairs = [
    ("release_date", "release_date"),
    ("explicit", "explicit"),
    ("languages", "languages"),
    ("resume_point", "resume_point"),
    ("uri", "uri"),
  ];
  keep(&mut p, v, &pairs);
  Some(p)
}

/// An account: `GET /me`, a playlist's owner.
pub fn user(v: &Value) -> Option<User> {
  let id = v.str("id")?;
  let mut u = User {
    url: Some(refs::url(Kind::User, &id)),
    name: v.str("display_name").unwrap_or_else(|| id.clone()),
    avatar: v.str("images.0.url"),
    raw: Some(v.clone()),
    id,
    ..User::default()
  };
  u.stats.followers = v.count("followers.total");
  for key in ["account_id", "product", "country", "uri"] {
    if let Some(value) = v.str(key) {
      u.extra.insert(key.into(), json!(value));
    }
  }
  Some(u)
}

/// A Spotify Connect device (`kind` its type: computer, smartphone,
/// speaker …), for `player devices`.
pub fn device(v: &Value) -> Option<Collection> {
  let id = v.str("id")?;
  let mut c = Collection {
    kind: v
      .str("type")
      .map(|t| t.to_ascii_lowercase())
      .unwrap_or_else(|| "device".into()),
    name: v.str("name").unwrap_or_default(),
    raw: Some(v.clone()),
    id,
    ..Collection::default()
  };
  for key in [
    "is_active",
    "volume_percent",
    "supports_volume",
    "is_private_session",
    "is_restricted",
  ] {
    let value = v.at(key);
    if !value.is_null() {
      c.extra.insert(key.into(), value.clone());
    }
  }
  Some(c)
}
