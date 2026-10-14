//! Songs, albums, playlists, artists and podcasts as posts.

use atp_core::{Media, Post, User, Value, ValueExt, json};

use super::{https, time, user};
use crate::refs;

/// The artists of a song or an album: `[{id, name}]`.
fn artists(v: &Value) -> Vec<Value> {
  let list = match v.list("ar") {
    [] => v.list("artists"),
    list => list,
  };
  list
    .iter()
    .filter_map(|a| Some(json!({"id": a.str("id"), "name": a.str("name")?})))
    .collect()
}

/// The artists as the author line: their names joined, the first one's id.
fn artist_author(list: &[Value]) -> Option<User> {
  let first = list.first()?;
  let id = first.str("id").filter(|id| id != "0");
  let names: Vec<String> = list.iter().filter_map(|a| a.str("name")).collect();
  Some(User {
    url: id.as_ref().map(refs::artist_url),
    id: id.unwrap_or_default(),
    name: names.join(" / "),
    ..User::default()
  })
}

/// Seconds of a duration in milliseconds.
fn seconds(v: &Value, paths: &[&str]) -> Option<f64> {
  paths
    .iter()
    .find_map(|p| v.f64(p))
    .filter(|ms| *ms > 0.0)
    .map(|ms| (ms / 1000.0).round())
}

fn cover(v: &Value, paths: &[&str]) -> Vec<Media> {
  v.first_str(paths)
    .map(|url| vec![Media::image(https(url))])
    .unwrap_or_default()
}

fn names(v: &Value, paths: &[&str]) -> Vec<String> {
  paths
    .iter()
    .flat_map(|p| v.list(p))
    .filter_map(|n| n.as_str().map(str::to_owned))
    .filter(|n| !n.is_empty())
    .collect()
}

/// A song (`kind` `song`): artists as the author, album, duration and
/// alias in `extra`, the album cover as its image. Never any audio.
pub fn song(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let album = match v.at("al") {
    Value::Null => v.at("album"),
    al => al,
  };
  let artists = artists(v);
  let mut p = Post {
    url: Some(refs::song_url(&id)),
    kind: "song".into(),
    title: v.str("name"),
    author: artist_author(&artists),
    created_at: time(v, &["publishTime"]).or_else(|| time(album, &["publishTime"])),
    media: cover(album, &["picUrl", "blurPicUrl"]),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  if let Some(secs) = seconds(v, &["dt", "duration"]) {
    p.extra.insert("duration".into(), json!(secs));
  }
  p.extra.insert("artists".into(), Value::Array(artists));
  if let Some(name) = album.str("name") {
    p.extra
      .insert("album".into(), json!({"id": album.str("id"), "name": name}));
  }
  let alias = names(v, &["alia", "alias", "tns", "transNames"]);
  if !alias.is_empty() {
    p.extra.insert("alias".into(), json!(alias));
  }
  Some(p)
}

/// An album (`kind` `album`).
pub fn album(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let artists = artists(v);
  let mut p = Post {
    url: Some(refs::album_url(&id)),
    kind: "album".into(),
    title: v.str("name"),
    text: v.str("description"),
    author: artist_author(&artists).or_else(|| artist_author(&[v.at("artist").clone()])),
    created_at: time(v, &["publishTime"]),
    media: cover(v, &["picUrl", "blurPicUrl"]),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  p.metrics.comments = v.count("info.commentCount");
  p.metrics.shares = v.count("info.shareCount");
  p.metrics.likes = v.count("info.likedCount");
  p.extra.insert("artists".into(), Value::Array(artists));
  for (key, path) in [
    ("track_count", "size"),
    ("company", "company"),
    ("type", "type"),
  ] {
    if let Some(value) = v.str(path) {
      p.extra.insert(key.into(), json!(value));
    }
  }
  Some(p)
}

/// A playlist or a chart (`kind` `playlist`).
pub fn playlist(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let mut p = Post {
    url: Some(refs::playlist_url(&id)),
    kind: "playlist".into(),
    title: v.str("name"),
    text: v.str("description"),
    author: user(v.at("creator")),
    created_at: time(v, &["createTime"]),
    updated_at: time(v, &["trackUpdateTime", "updateTime"]),
    media: cover(v, &["coverImgUrl", "picUrl"]),
    tags: names(v, &["tags"]),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  p.metrics.views = v.count("playCount");
  p.metrics.favorites = v.first_count(&["subscribedCount", "bookCount"]);
  p.metrics.comments = v.count("commentCount");
  p.metrics.shares = v.count("shareCount");
  if let Some(n) = v.u64("trackCount") {
    p.extra.insert("track_count".into(), json!(n));
  }
  if let Some(every) = v.str("updateFrequency") {
    p.extra.insert("updates".into(), json!(every));
  }
  if v.i64("privacy").is_some_and(|x| x != 0) {
    p.extra.insert("private".into(), json!(true));
  }
  if let Some(subscribed) = v.bool("subscribed") {
    p.extra.insert("subscribed".into(), json!(subscribed));
  }
  Some(p)
}

/// An artist (`kind` `artist`).
pub fn artist(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let mut p = Post {
    url: Some(refs::artist_url(&id)),
    kind: "artist".into(),
    title: v.str("name"),
    text: v.str("briefDesc"),
    media: cover(v, &["picUrl", "img1v1Url", "cover"]),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  for (key, path) in [
    ("albums", "albumSize"),
    ("songs", "musicSize"),
    ("mvs", "mvSize"),
    ("account", "accountId"),
  ] {
    if let Some(n) = v.u64(path).filter(|n| *n > 0) {
      p.extra.insert(key.into(), json!(n));
    }
  }
  let alias = names(v, &["alias", "trans"]);
  if !alias.is_empty() {
    p.extra.insert("alias".into(), json!(alias));
  }
  Some(p)
}

/// A podcast episode (节目, `kind` `episode`). The audio stays upstream.
pub fn episode(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let mut p = Post {
    url: Some(refs::episode_url(&id)),
    kind: "episode".into(),
    title: v.str("name"),
    text: v.str("description"),
    author: user(v.at("dj")),
    created_at: time(v, &["createTime", "scheduledPublishTime"]),
    media: cover(v, &["coverUrl", "blurCoverUrl"]),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  p.metrics.views = v.count("listenerCount");
  p.metrics.likes = v.count("likedCount");
  p.metrics.comments = v.count("commentCount");
  p.metrics.shares = v.count("shareCount");
  if let Some(secs) = seconds(v, &["duration", "mainSong.duration"]) {
    p.extra.insert("duration".into(), json!(secs));
  }
  if let Some(name) = v.str("radio.name") {
    p.extra.insert(
      "podcast".into(),
      json!({"id": v.str("radio.id"), "name": name}),
    );
  }
  if let Some(n) = v.u64("serialNum") {
    p.extra.insert("number".into(), json!(n));
  }
  Some(p)
}

/// A podcast show (播客 / 电台, `kind` `podcast`).
pub fn podcast(v: &Value) -> Option<Post> {
  let id = v.str("id")?;
  let mut p = Post {
    url: Some(refs::podcast_url(&id)),
    kind: "podcast".into(),
    title: v.str("name"),
    text: v.first_str(&["desc", "rcmdText"]),
    author: user(v.at("dj")),
    created_at: time(v, &["createTime"]),
    media: cover(v, &["picUrl", "intervenePicUrl"]),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  p.metrics.favorites = v.count("subCount");
  p.metrics.shares = v.count("shareCount");
  p.metrics.likes = v.count("likedCount");
  if let Some(n) = v.u64("programCount") {
    p.extra.insert("episodes".into(), json!(n));
  }
  if let Some(category) = v.str("category") {
    p.extra.insert("category".into(), json!(category));
  }
  Some(p)
}
