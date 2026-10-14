//! Lyrics (歌词) and their translation. The gateway sends LRC text base64
//! encoded (some answers hex encoded); it is given back as plain text or
//! LRC, each translated line right under its original.

use atp_core::music::Lyrics;
use atp_core::{Error, Post, Result, Value, ValueExt, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::api::{Api, Call};
use crate::refs::Key;
use crate::songs;

/// The lyrics call for a song (to send alone or with others): plain LRC,
/// with the translation.
pub fn call<'a>(api: &'a Api, key: &Key) -> Call<'a> {
  let call = api
    .call("music.musichallSong.PlayLyricInfo", "GetPlayLyricInfo")
    .arg("trans", 1)
    .arg("roma", 0)
    .arg("crypt", 0)
    .arg("type", 0);
  match key {
    Key::Mid(mid) => call.arg("songMID", mid.as_str()),
    Key::Id(id) => call.arg("songID", *id),
  }
}

/// The lyrics in an answer of [`call`].
pub fn parse(v: &Value) -> Lyrics {
  Lyrics::from_lrc(
    &decode(&v.str("lyric").unwrap_or_default()),
    &decode(&v.str("trans").unwrap_or_default()),
  )
}

/// Base64 or hex encoded UTF-8 text, decoded; anything else as it came.
pub fn decode(text: &str) -> String {
  let text = text.trim();
  if text.is_empty() {
    return String::new();
  }
  if text.len().is_multiple_of(2)
    && text.bytes().all(|b| b.is_ascii_hexdigit())
    && let Some(plain) = from_hex(text)
  {
    return plain;
  }
  STANDARD
    .decode(text)
    .ok()
    .and_then(|bytes| String::from_utf8(bytes).ok())
    .unwrap_or_else(|| text.to_owned())
}

fn from_hex(text: &str) -> Option<String> {
  let bytes = (0..text.len())
    .step_by(2)
    .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
    .collect::<Option<Vec<u8>>>()?;
  String::from_utf8(bytes).ok()
}

/// `lyrics SONG`: the song with only its lyrics as the text.
pub async fn command(api: &Api, key: &Key, lrc: bool, original: bool) -> Result<Post> {
  let tracks = songs::tracks_call(api, &[key]);
  let mut data = api.batch(vec![tracks, call(api, key)]).await?;
  let raw = data.pop().unwrap_or_default();
  let mut song = data
    .pop()
    .and_then(|v| v.list("tracks").iter().find_map(crate::parse::song))
    .ok_or_else(|| Error::not_found(format!("no song {}", songs::show(key))))?;
  let lyrics = parse(&raw);
  let translation = !original;
  let text = if lrc {
    lyrics.lrc(translation)
  } else {
    lyrics.plain(translation)
  };
  song.text =
    Some(text.ok_or_else(|| Error::not_found(format!("song {} has no lyrics", songs::show(key))))?);
  song.kind = "lyrics".into();
  song.media.clear();
  song
    .extra
    .insert("format".into(), json!(if lrc { "lrc" } else { "plain" }));
  if lyrics.translated() && translation {
    song.extra.insert("translated".into(), json!(true));
  }
  song.raw = Some(raw);
  Ok(song)
}
