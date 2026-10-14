//! Songs, albums, playlists, singers and charts as posts.

use atp_core::text::html_to_text;
use atp_core::{Media, Post, User, Value, ValueExt, json};

use super::{https, time, user};
use crate::refs;

/// The singers of a song or an album: `[{id, name}]` (ids are mids).
fn artists(list: &[Value]) -> Vec<Value> {
  list
    .iter()
    .filter_map(|a| {
      Some(
        json!({"id": a.first_str(&["mid", "singerMID"]), "name": a.first_str(&["name", "title"])?}),
      )
    })
    .collect()
}

/// The singers as the author line: their names joined, the first one's mid.
fn artist_author(list: &[Value]) -> Option<User> {
  let first = list.first()?;
  let id = first.str("id");
  let names: Vec<String> = list.iter().filter_map(|a| a.str("name")).collect();
  Some(User {
    url: id.as_deref().map(refs::singer_url),
    id: id.unwrap_or_default(),
    name: names.join(" / "),
    ..User::default()
  })
}

fn image(v: &Value, paths: &[&str]) -> Vec<Media> {
  v.first_str(paths)
    .filter(|u| u.starts_with("http"))
    .map(|url| vec![Media::image(https(url))])
    .unwrap_or_default()
}

/// The cover of a song's album, or its first singer's picture.
fn song_cover(album: &Value, singers: &[Value]) -> Vec<Media> {
  let album = album
    .first_str(&["pmid", "mid"])
    .filter(|m| m.len() > 2 && m != "空");
  let url = match album {
    Some(mid) => refs::album_cover(&mid),
    None => match singers.first().and_then(|s| s.str("id")) {
      Some(mid) => refs::singer_picture(&mid),
      None => return Vec::new(),
    },
  };
  vec![Media::image(url)]
}

/// A song (`kind` `song`, id its mid): singers as the author, album,
/// duration and number in `extra`, the album cover as its image. Never
/// any audio.
pub fn song(v: &Value) -> Option<Post> {
  let mid = v.str("mid")?;
  let album = v.at("album");
  let singers = artists(v.list("singer"));
  let mut p = Post {
    url: Some(refs::song_url(&mid)),
    kind: "song".into(),
    title: v.first_str(&["title", "name"]),
    author: artist_author(&singers),
    created_at: time(v, &["time_public"]).or_else(|| time(album, &["time_public"])),
    media: song_cover(album, &singers),
    raw: Some(v.clone()),
    id: mid,
    ..Post::default()
  };
  if let Some(secs) = v.u64("interval").filter(|s| *s > 0) {
    p.extra.insert("duration".into(), json!(secs));
  }
  p.extra.insert("artists".into(), Value::Array(singers));
  if let Some(name) = album.first_str(&["name", "title"]) {
    p.extra.insert(
      "album".into(),
      json!({"id": album.str("mid"), "name": name}),
    );
  }
  if let Some(id) = v.u64("id") {
    p.extra.insert("song_id".into(), json!(id));
  }
  for (key, path) in [("subtitle", "subtitle"), ("lyrics_match", "lyric")] {
    if let Some(text) = v
      .str(path)
      .map(|t| t.trim().to_owned())
      .filter(|t| !t.is_empty())
    {
      p.extra.insert(key.into(), json!(text));
    }
  }
  Some(p)
}

/// An album (`kind` `album`, id its mid), from its page (`basicInfo` …),
/// a search or a list of collected albums.
pub fn album(v: &Value) -> Option<Post> {
  let b = match v.at("basicInfo") {
    Value::Null => v,
    info => info,
  };
  let mid = b.first_str(&["albumMid", "albumMID", "mid"])?;
  let singers = artists(
    [
      v.list("singer.singerList"),
      v.list("singer_list"),
      v.list("v_singer"),
    ]
    .into_iter()
    .find(|l| !l.is_empty())
    .unwrap_or_default(),
  );
  let mut p = Post {
    url: Some(refs::album_url(&mid)),
    kind: "album".into(),
    title: b.first_str(&["albumName", "name"]),
    text: b.str("desc"),
    author: artist_author(&singers),
    created_at: time(b, &["publishDate", "publicTime", "pubtime"]),
    media: vec![Media::image(refs::album_cover(
      &b.str("pmid").unwrap_or_else(|| mid.clone()),
    ))],
    raw: Some(v.clone()),
    id: mid,
    ..Post::default()
  };
  p.extra.insert("artists".into(), Value::Array(singers));
  for (key, path) in [
    ("album_id", "albumID"),
    ("album_id", "id"),
    ("track_count", "song_count"),
    ("track_count", "songnum"),
    ("type", "albumType"),
    ("language", "language"),
    ("translation", "tranName"),
  ] {
    if let Some(value) = b.str(path).filter(|_| !p.extra.contains_key(key)) {
      p.extra.insert(key.into(), json!(value));
    }
  }
  if let Some(company) = v.str("company.name") {
    p.extra.insert("company".into(), json!(company));
  }
  Some(p)
}

/// A playlist (歌单, `kind` `playlist`): its page's `dirinfo`, a search
/// result, or an entry of someone's created or collected playlists.
pub fn playlist(v: &Value) -> Option<Post> {
  let id = v
    .first_str(&["tid", "dissid", "id"])
    .filter(|id| id != "0")?;
  let mut p = Post {
    url: Some(refs::playlist_url(&id)),
    kind: "playlist".into(),
    title: v.first_str(&["title", "dissname", "name", "diss_name", "dirName"]),
    text: v.first_str(&["desc", "introduction"]),
    author: playlist_owner(v),
    created_at: time(v, &["ctime", "createTime", "createtime"]),
    updated_at: time(v, &["mtime", "updateTime"]),
    media: image(v, &["picurl", "imgurl", "logo", "diss_cover", "picUrl"]),
    tags: v.list("tag").iter().filter_map(|t| t.str("name")).collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  p.metrics.views = v.first_count(&["listennum", "listen_num", "play_cnt"]);
  p.metrics.favorites = v.first_count(&["ordernum", "fav_cnt"]);
  p.metrics.comments = v.count("comment_cnt");
  if let Some(n) = v.first_count(&["songnum", "song_count", "song_cnt", "songNum"]) {
    p.extra.insert("track_count".into(), json!(n));
  }
  if let Some(dir) = v.first_count(&["dirid", "dirId"]) {
    p.extra.insert("dir_id".into(), json!(dir));
  }
  if v.first_count(&["dir_show", "dirShow"]) == Some(0) {
    p.extra.insert("private".into(), json!(true));
  }
  Some(p)
}

/// Who made a playlist: its `creator`, or the owner fields of a list entry.
fn playlist_owner(v: &Value) -> Option<User> {
  if let Some(creator) = user(v.at("creator")) {
    return Some(User {
      raw: None,
      ..creator
    });
  }
  let id = v
    .first_str(&["encrypt_uin", "uin"])
    .filter(|u| u.parse::<u64>().is_err())?;
  Some(User {
    url: Some(refs::user_url(&id)),
    name: v
      .first_str(&["host_nick", "nickname", "nick"])
      .unwrap_or_default(),
    id,
    ..User::default()
  })
}

/// A singer (`kind` `artist`, id the mid): their page or a search result.
pub fn artist(v: &Value) -> Option<Post> {
  let mid = v.first_str(&["basic_info.singer_mid", "singerMID", "singer_mid", "mid"])?;
  let mut p = Post {
    url: Some(refs::singer_url(&mid)),
    kind: "artist".into(),
    title: v.first_str(&["basic_info.name", "singerName", "name"]),
    text: v.str("ex_info.desc"),
    media: vec![Media::image(refs::singer_picture(
      &v.str("basic_info.singer_pmid")
        .unwrap_or_else(|| mid.clone()),
    ))],
    raw: Some(v.clone()),
    id: mid,
    ..Post::default()
  };
  for (key, path) in [
    ("songs", "songNum"),
    ("albums", "albumNum"),
    ("mvs", "mvNum"),
  ] {
    if let Some(n) = v.u64(path).filter(|n| *n > 0) {
      p.extra.insert(key.into(), json!(n));
    }
  }
  for (key, path) in [
    ("singer_id", "basic_info.singer_id"),
    ("singer_id", "singerID"),
    ("foreign_name", "ex_info.foreign_name"),
    ("birthday", "ex_info.birthday"),
  ] {
    if let Some(value) = v.str(path).filter(|_| !p.extra.contains_key(key)) {
      p.extra.insert(key.into(), json!(value));
    }
  }
  Some(p)
}

/// A chart (榜单, `kind` `toplist`, id its `topId`).
pub fn toplist(v: &Value) -> Option<Post> {
  let id = v.str("topId")?;
  let mut p = Post {
    url: Some(refs::toplist_url(&id)),
    kind: "toplist".into(),
    title: v.str("title"),
    text: v.str("intro").map(|t| html_to_text(&t)),
    updated_at: time(v, &["updateTime"]),
    media: image(v, &["frontPicUrl", "headPicUrl", "mbFrontPicUrl"]),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  p.metrics.views = v.count("listenNum");
  if let Some(n) = v.u64("totalNum") {
    p.extra.insert("track_count".into(), json!(n));
  }
  for (key, path) in [("period", "period"), ("updates", "updateTips")] {
    if let Some(value) = v.str(path) {
      p.extra.insert(key.into(), json!(value));
    }
  }
  Some(p)
}
