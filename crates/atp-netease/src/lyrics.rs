//! Lyrics (歌词) and their translation: fetched as LRC, given back as plain
//! text or LRC, each translated line right under its original.

use atp_core::music;
use atp_core::{Error, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::songs;

/// The lyrics of a song and the answer they came in.
pub struct Lyrics {
  text: music::Lyrics,
  raw: Value,
}

pub async fn fetch(api: &Api, id: u64) -> Result<Lyrics> {
  let v = api
    .web("song/lyric")
    .arg("id", id)
    .arg("lv", -1)
    .arg("tv", -1)
    .arg("rv", -1)
    .arg("kv", -1)
    .send()
    .await?;
  let text = music::Lyrics::from_lrc(
    &v.str("lrc.lyric").unwrap_or_default(),
    &v.str("tlyric.lyric").unwrap_or_default(),
  );
  Ok(Lyrics { text, raw: v })
}

impl Lyrics {
  pub fn translated(&self) -> bool {
    self.text.translated()
  }

  /// The words without times, translations under their lines; `None` for
  /// a song without lyrics.
  pub fn plain(&self, translation: bool) -> Option<String> {
    self.text.plain(translation)
  }

  /// LRC lines, a translation as a second line with the same time.
  pub fn lrc(&self, translation: bool) -> Option<String> {
    self.text.lrc(translation)
  }

  /// Who wrote the lyrics and the translation down, as NetEase credits them.
  pub fn contributors(&self) -> Option<Value> {
    let lyrics = self.raw.str("lyricUser.nickname");
    let translation = self.raw.str("transUser.nickname");
    (lyrics.is_some() || translation.is_some())
      .then(|| json!({"lyrics": lyrics, "translation": translation}))
  }
}

/// `lyrics SONG`: the song with only its lyrics as the text.
pub async fn command(api: &Api, id: u64, lrc: bool, original: bool) -> Result<Post> {
  let mut song = songs::details(api, &[id.to_string()])
    .await?
    .pop()
    .ok_or_else(|| Error::not_found(format!("no song {id}")))?;
  let lyrics = fetch(api, id).await?;
  let translation = !original;
  let text = if lrc {
    lyrics.lrc(translation)
  } else {
    lyrics.plain(translation)
  };
  song.text = Some(text.ok_or_else(|| Error::not_found(format!("song {id} has no lyrics")))?);
  song.kind = "lyrics".into();
  song.media.clear();
  song
    .extra
    .insert("format".into(), json!(if lrc { "lrc" } else { "plain" }));
  if lyrics.translated() && translation {
    song.extra.insert("translated".into(), json!(true));
  }
  song.raw = Some(lyrics.raw);
  Ok(song)
}
