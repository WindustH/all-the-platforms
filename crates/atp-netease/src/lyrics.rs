//! Lyrics (歌词) and their translation: fetched as LRC, given back as plain
//! text or LRC, each translated line right under its original.

use std::collections::BTreeMap;

use atp_core::{Error, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::songs;

/// One timed line.
#[derive(Debug, Clone)]
struct Line {
  ms: u64,
  text: String,
}

/// The lyrics of a song.
#[derive(Debug, Default)]
pub struct Lyrics {
  lines: Vec<Line>,
  /// Translated lines by the time of their original.
  translation: BTreeMap<u64, String>,
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
  let translation = parse_lrc(&v.str("tlyric.lyric").unwrap_or_default())
    .into_iter()
    .filter(|l| !l.text.is_empty())
    .map(|l| (l.ms, l.text))
    .collect();
  Ok(Lyrics {
    lines: parse_lrc(&v.str("lrc.lyric").unwrap_or_default()),
    translation,
    raw: v,
  })
}

impl Lyrics {
  pub fn translated(&self) -> bool {
    !self.translation.is_empty()
  }

  /// The words without times, translations under their lines; `None` for
  /// a song without lyrics.
  pub fn plain(&self, translation: bool) -> Option<String> {
    let mut out: Vec<&str> = Vec::new();
    for line in &self.lines {
      if line.text.is_empty() && out.last().is_none_or(|l| l.is_empty()) {
        continue;
      }
      out.push(&line.text);
      if let Some(t) = self.translation.get(&line.ms).filter(|_| translation) {
        out.push(t);
      }
    }
    let text = out.join("\n").trim().to_owned();
    (!text.is_empty()).then_some(text)
  }

  /// LRC lines, a translation as a second line with the same time.
  pub fn lrc(&self, translation: bool) -> Option<String> {
    let mut out = Vec::new();
    for line in &self.lines {
      out.push(format!("{}{}", stamp(line.ms), line.text));
      if let Some(t) = self.translation.get(&line.ms).filter(|_| translation) {
        out.push(format!("{}{t}", stamp(line.ms)));
      }
    }
    (!out.is_empty()).then(|| out.join("\n"))
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

/// Timed lines of LRC text (also the JSON credit lines newer lyrics start
/// with), in time order; tags such as `[ar:…]` are dropped.
fn parse_lrc(text: &str) -> Vec<Line> {
  let mut lines = Vec::new();
  for raw in text.lines().map(str::trim) {
    if raw.starts_with('{') {
      lines.extend(credit_line(raw));
      continue;
    }
    let mut rest = raw;
    let mut times = Vec::new();
    while let Some(tag) = rest.strip_prefix('[').and_then(|r| r.split_once(']')) {
      let Some(ms) = parse_stamp(tag.0) else { break };
      times.push(ms);
      rest = tag.1;
    }
    let text = rest.trim().to_owned();
    lines.extend(times.into_iter().map(|ms| Line {
      ms,
      text: text.clone(),
    }));
  }
  lines.sort_by_key(|l| l.ms);
  lines
}

/// `{"t":0,"c":[{"tx":"作词: "},{"tx":"方文山"}]}`.
fn credit_line(raw: &str) -> Option<Line> {
  let v: Value = serde_json::from_str(raw).ok()?;
  let text: String = v.list("c").iter().filter_map(|c| c.str("tx")).collect();
  Some(Line {
    ms: v.u64("t").unwrap_or(0),
    text: text.trim().to_owned(),
  })
}

/// `mm:ss`, `mm:ss.xx`, `mm:ss.xxx` or `mm:ss:xx` -> milliseconds.
fn parse_stamp(tag: &str) -> Option<u64> {
  let (min, rest) = tag.split_once(':')?;
  let min: u64 = min.trim().parse().ok()?;
  let (sec, frac) = match rest.split_once(['.', ':']) {
    Some((s, f)) => (s, f),
    None => (rest, "0"),
  };
  let sec: u64 = sec.trim().parse().ok()?;
  let digits = frac.trim();
  let frac: u64 = digits.parse().ok()?;
  let ms = match digits.len() {
    1 => frac * 100,
    2 => frac * 10,
    _ => frac / 10u64.pow(digits.len() as u32 - 3),
  };
  Some(min * 60_000 + sec * 1000 + ms)
}

/// `[mm:ss.xx]`.
fn stamp(ms: u64) -> String {
  format!(
    "[{:02}:{:02}.{:02}]",
    ms / 60_000,
    ms / 1000 % 60,
    ms % 1000 / 10
  )
}
