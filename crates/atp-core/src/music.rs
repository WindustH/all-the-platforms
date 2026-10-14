//! What music platforms share: lyrics in LRC with their translation, and
//! track lists inside a post (an album, a playlist, a chart, a show).

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::json::ValueExt;
use crate::model::Post;
use crate::text::fmt_duration;

/// One timed line.
#[derive(Debug, Clone)]
struct Line {
  ms: u64,
  text: String,
}

/// The lyrics of a song: timed lines and the translated lines by the time
/// of their original.
#[derive(Debug, Default)]
pub struct Lyrics {
  lines: Vec<Line>,
  translation: BTreeMap<u64, String>,
}

impl Lyrics {
  /// Lyrics from LRC text and the LRC text of their translation (empty
  /// when there is none). Translated lines that are empty or only `//`
  /// (a line left untranslated) are dropped.
  pub fn from_lrc(original: &str, translation: &str) -> Self {
    let translation = parse_lrc(translation)
      .into_iter()
      .filter(|l| !l.text.is_empty() && l.text != "//")
      .map(|l| (l.ms, l.text))
      .collect();
    Self {
      lines: parse_lrc(original),
      translation,
    }
  }

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
}

/// Timed lines of LRC text (also the JSON credit lines some services start
/// their lyrics with), in time order; tags such as `[ar:…]` are dropped.
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

/// Numbered lines for a post's text: `1. Title — Artists (3:45)`.
pub fn track_list(heading: &str, tracks: &[Post]) -> String {
  let mut out = format!("{heading}:");
  for (i, t) in tracks.iter().enumerate() {
    out.push_str(&format!(
      "\n{:>3}. {}",
      i + 1,
      t.title.as_deref().unwrap_or("?")
    ));
    if let Some(a) = &t.author {
      out.push_str(&format!(" — {}", a.name));
    }
    if let Some(secs) = t.extra.get("duration").and_then(Value::as_f64) {
      out.push_str(&format!(" ({})", fmt_duration(secs)));
    }
  }
  out
}

/// The tracks for `extra.tracks`: id, title, artists, duration and link.
pub fn tracks_extra(tracks: &[Post]) -> Value {
  Value::Array(
    tracks
      .iter()
      .map(|t| {
        json!({
          "id": t.id,
          "title": t.title,
          "artists": t.author.as_ref().map(|a| a.name.clone()),
          "duration": t.extra.get("duration"),
          "url": t.url,
        })
      })
      .collect(),
  )
}

/// Add a track list to a post's text and `extra.tracks`.
pub fn with_tracks(post: &mut Post, heading: &str, tracks: &[Post]) {
  if tracks.is_empty() {
    return;
  }
  let list = track_list(heading, tracks);
  post.text = Some(match post.text.take().filter(|t| !t.trim().is_empty()) {
    Some(text) => format!("{}\n\n{list}", text.trim_end()),
    None => list,
  });
  post.extra.insert("tracks".into(), tracks_extra(tracks));
}
