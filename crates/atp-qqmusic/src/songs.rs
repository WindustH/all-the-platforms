//! Songs: details by mid or number, one song with its lyrics, and liking
//! (喜欢) a song, which puts it into the account's 我喜欢 playlist.

use atp_core::{Action, Error, Post, Result, Value, ValueExt, json};

use crate::api::{Api, Call};
use crate::refs::Key;
use crate::{lyrics, parse};

/// Most songs one track-info call takes.
const BATCH: usize = 100;
/// The `dirId` of 我喜欢, every account's liked songs.
pub const LIKED_DIR: u64 = 201;

/// The track-info call for songs that are all mids or all numbers (to
/// send alone or with others).
pub fn tracks_call<'a>(api: &'a Api, keys: &[&Key]) -> Call<'a> {
  let zeros = Value::Array(vec![json!(0); keys.len()]);
  let call = api
    .call("music.trackInfo.UniformRuleCtrl", "CgiGetTrackInfo")
    .arg("types", zeros.clone())
    .arg("modify_stamp", zeros)
    .arg("ctx", 0)
    .arg("client", 1);
  let mids: Vec<Value> = keys
    .iter()
    .filter_map(|k| match k {
      Key::Mid(mid) => Some(json!(mid)),
      Key::Id(_) => None,
    })
    .collect();
  if mids.is_empty() {
    let ids: Vec<Value> = keys
      .iter()
      .filter_map(|k| match k {
        Key::Id(id) => Some(json!(id)),
        Key::Mid(_) => None,
      })
      .collect();
    call.arg("ids", Value::Array(ids))
  } else {
    call.arg("mids", Value::Array(mids))
  }
}

/// Songs by mid or number, in the order asked (unknown ones left out).
pub async fn details(api: &Api, keys: &[Key]) -> Result<Vec<Post>> {
  let mut found: Vec<Post> = Vec::with_capacity(keys.len());
  for chunk in keys.chunks(BATCH) {
    let (mids, ids): (Vec<&Key>, Vec<&Key>) = chunk.iter().partition(|k| matches!(k, Key::Mid(_)));
    let calls: Vec<Call> = [mids, ids]
      .iter()
      .filter(|group| !group.is_empty())
      .map(|group| tracks_call(api, group))
      .collect();
    for data in api.batch(calls).await? {
      found.extend(data.list("tracks").iter().filter_map(parse::song));
    }
  }
  Ok(
    keys
      .iter()
      .filter_map(|key| found.iter().find(|song| is(song, key)).cloned())
      .collect(),
  )
}

/// Whether `song` is the one `key` names.
fn is(song: &Post, key: &Key) -> bool {
  match key {
    Key::Mid(mid) => song.id == *mid,
    Key::Id(id) => song.extra.get("song_id").and_then(Value::as_u64) == Some(*id),
  }
}

/// One song, or `not_found`.
pub async fn one(api: &Api, key: &Key) -> Result<Post> {
  details(api, std::slice::from_ref(key))
    .await?
    .pop()
    .ok_or_else(|| Error::not_found(format!("no song {}", show(key))))
}

/// A key as it was given.
pub fn show(key: &Key) -> String {
  match key {
    Key::Mid(mid) => mid.clone(),
    Key::Id(id) => id.to_string(),
  }
}

/// The number and the type of a song, as writes and comments want them.
pub fn number(song: &Post) -> Result<(u64, u64)> {
  let raw = song.raw.as_ref().cloned().unwrap_or_default();
  let id = raw
    .u64("id")
    .ok_or_else(|| Error::upstream("QQ Music sent a song without its number"))?;
  Ok((id, raw.u64("type").unwrap_or(0)))
}

/// One song with its lyrics (and their translation) as the text, and what
/// the song page says about it (introduction, genre, language, company).
pub async fn read(api: &Api, key: &Key) -> Result<Post> {
  let detail = api.call("music.pf_song_detail_svr", "get_song_detail_yqq");
  let detail = match key {
    Key::Mid(mid) => detail.arg("song_mid", mid.as_str()),
    Key::Id(id) => detail.arg("song_id", *id),
  };
  let mut data = api.batch(vec![detail, lyrics::call(api, key)]).await?;
  let lyrics = lyrics::parse(&data.pop().unwrap_or_default());
  let detail = data.pop().unwrap_or_default();
  let mut song = parse::song(detail.at("track_info"))
    .ok_or_else(|| Error::not_found(format!("no song {}", show(key))))?;
  song.text = lyrics.plain(true);
  if lyrics.translated() {
    song.extra.insert("lyrics_translated".into(), json!(true));
  }
  for (key, path) in [
    ("intro", "info.intro"),
    ("genre", "info.genre"),
    ("language", "info.lan"),
    ("company", "info.company"),
  ] {
    let values: Vec<String> = detail
      .list(&format!("{path}.content"))
      .iter()
      .filter_map(|c| c.str("value"))
      .collect();
    if !values.is_empty() {
      song.extra.insert(key.into(), json!(values.join(" / ")));
    }
  }
  song.raw = Some(detail);
  Ok(song)
}

/// Add songs to one of the account's playlists by its `dirId`, or take
/// them out.
pub async fn change(api: &Api, dir: u64, songs: &[Post], remove: bool) -> Result<()> {
  let mut list = Vec::with_capacity(songs.len());
  for song in songs {
    let (id, kind) = number(song)?;
    list.push(json!({"songId": id, "songType": kind}));
  }
  let method = if remove { "DelSonglist" } else { "AddSonglist" };
  api
    .call("music.musicasset.PlaylistDetailWrite", method)
    .arg("dirId", dir)
    .arg("v_songInfo", Value::Array(list))
    .write()
    .send()
    .await?;
  Ok(())
}

/// Like a song (喜欢: into 我喜欢), or take it back.
pub async fn like(api: &Api, key: &Key, undo: bool) -> Result<Action> {
  api.require_login()?;
  let song = one(api, key).await?;
  change(api, LIKED_DIR, std::slice::from_ref(&song), undo).await?;
  let action = if undo { "unlike" } else { "like" };
  Ok(Action::done(action, song.id.clone()).with_url(song.url.unwrap_or_default()))
}
