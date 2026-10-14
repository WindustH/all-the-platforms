//! Songs: details by id (in batches), one song with its lyrics, 红心.

use atp_core::{Action, Error, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{lyrics, parse};

/// Most songs one `song/detail` call takes.
const BATCH: usize = 500;

/// Songs by id, in the order asked (unknown ids are left out).
pub async fn details(api: &Api, ids: &[String]) -> Result<Vec<Post>> {
  let mut songs = Vec::with_capacity(ids.len());
  for chunk in ids.chunks(BATCH) {
    let c: Vec<Value> = chunk
      .iter()
      .filter_map(|id| id.parse::<u64>().ok())
      .map(|id| json!({"id": id}))
      .collect();
    let v = api
      .web("v3/song/detail")
      .arg("c", Value::Array(c).to_string())
      .send()
      .await?;
    songs.extend(v.list("songs").iter().filter_map(parse::song));
  }
  Ok(songs)
}

/// One song with its lyrics (and their translation) as the text.
pub async fn read(api: &Api, id: u64) -> Result<Post> {
  let mut song = details(api, &[id.to_string()])
    .await?
    .pop()
    .ok_or_else(|| Error::not_found(format!("no song {id}")))?;
  let lyrics = lyrics::fetch(api, id).await?;
  song.text = lyrics.plain(true);
  if let Some(by) = lyrics.contributors() {
    song.extra.insert("lyrics_by".into(), by);
  }
  if lyrics.translated() {
    song.extra.insert("lyrics_translated".into(), json!(true));
  }
  Ok(song)
}

/// 红心 a song (add it to 我喜欢的音乐), or take it back.
pub async fn like(api: &Api, id: u64, undo: bool) -> Result<Action> {
  api
    .web("radio/like")
    .arg("alg", "itembased")
    .arg("trackId", id)
    .arg("like", !undo)
    .arg("time", "3")
    .write()
    .send()
    .await?;
  let action = if undo { "unlike" } else { "like" };
  Ok(Action::done(action, id.to_string()))
}
