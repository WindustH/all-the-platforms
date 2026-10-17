//! The text fields of an item (`text`, `text_extra`, `challenges` ...) as
//! the page derives them from its editor: hashtags as `#name` spans,
//! offsets in UTF-16 units (JavaScript string lengths), an image post's
//! title joined with `。`, an article's topics counted in code points.

use atp_core::{Value, json};

use super::{Tag, Words};

/// `text_extra` types.
const HASHTAG: i64 = 1;
const IMAGE_TITLE: i64 = 7;
const IMAGE_TITLE_DIVIDER: i64 = 8;

/// The text fields the publish form derives from its editor.
#[derive(Debug)]
pub struct Text {
  pub text: String,
  pub caption: String,
  pub title: String,
  pub extra: Value,
  pub challenges: Value,
  pub source: String,
}

/// The editor's content: the body, then each tag as `#name`, with the
/// hashtag spans the editor records (`start`, `end` in UTF-16 units).
fn editor<'t>(body: &str, tags: &'t [Tag]) -> (String, Vec<(usize, usize, &'t Tag)>) {
  let mut text = body.trim().to_owned();
  let mut spans = Vec::new();
  for tag in tags {
    if !text.is_empty() {
      text.push(' ');
    }
    let start = utf16(&text);
    text.push('#');
    text.push_str(&tag.name);
    spans.push((start, utf16(&text), tag));
  }
  (text, spans)
}

/// The text of a video (`eA` with a title): `title caption`, hashtag
/// offsets moved past the title, `caption_*` offsets within the caption.
pub fn video_text(words: Words) -> Text {
  let (title, tags) = (words.title.trim(), words.tags);
  let (caption, spans) = editor(words.body, tags);
  let shift = if title.is_empty() {
    0
  } else {
    utf16(title) + 1
  };
  let extra: Vec<Value> = spans
    .iter()
    .map(|(start, end, tag)| hashtag(tag, start + shift, end + shift, *start, *end))
    .collect();
  let text = if title.is_empty() {
    caption.clone()
  } else {
    format!("{title} {caption}")
  };
  Text {
    text,
    caption,
    title: title.to_owned(),
    extra: Value::Array(extra),
    challenges: challenges(tags),
    source: sources(tags),
  }
}

/// The text of an image post (`eA`, then the title joined with `。`).
pub fn image_text(words: Words) -> Text {
  let (title, tags) = (words.title.trim(), words.tags);
  let (caption, spans) = editor(words.body, tags);
  let mut prefix = title.to_owned();
  let mut extra = Vec::new();
  if !title.is_empty() {
    let len = utf16(title);
    extra.push(span(0, len, IMAGE_TITLE));
    if !title.ends_with('。') {
      prefix.push('。');
      extra.push(span(len, len + 1, IMAGE_TITLE_DIVIDER));
    }
  }
  let shift = utf16(&prefix);
  extra.extend(
    spans
      .iter()
      .map(|(start, end, tag)| hashtag(tag, start + shift, end + shift, *start, *end)),
  );
  Text {
    text: format!("{prefix}{caption}"),
    caption,
    title: title.to_owned(),
    extra: Value::Array(extra),
    challenges: challenges(tags),
    source: sources(tags),
  }
}

/// An article's `text_extra`: the title span, then each topic after it
/// (offsets in code points, `caption_*` without the title).
pub fn article_extra(title: &str, tags: &[Tag]) -> Value {
  if tags.is_empty() {
    return json!([]);
  }
  let len = title.chars().count();
  let mut extra = vec![span(0, len, IMAGE_TITLE)];
  let (mut at, mut caption_at) = (len + 1, 0);
  for tag in tags {
    let n = tag.name.chars().count();
    extra.push(json!({
      "hashtag_id": tag.id,
      "hashtag_name": tag.name,
      "type": HASHTAG,
      "start": at,
      "end": at + n + 1,
      "caption_start": caption_at,
      "caption_end": caption_at + n + 1,
    }));
    at += n + 2;
    caption_at += n + 2;
  }
  Value::Array(extra)
}

fn hashtag(tag: &Tag, start: usize, end: usize, caption_start: usize, caption_end: usize) -> Value {
  json!({
    "start": start,
    "end": end,
    "type": HASHTAG,
    "hashtag_name": tag.name,
    "hashtag_id": tag.id,
    "user_id": "",
    "caption_start": caption_start,
    "caption_end": caption_end,
  })
}

fn span(start: usize, end: usize, kind: i64) -> Value {
  json!({ "start": start, "end": end, "hashtag_id": 0, "hashtag_name": "", "type": kind })
}

/// Ids of the hashtags Douyin already knows.
fn challenges(tags: &[Tag]) -> Value {
  json!(
    tags
      .iter()
      .filter(|t| t.id != 0)
      .map(|t| t.id)
      .collect::<Vec<_>>()
  )
}

/// How each hashtag was added: typed (`search`).
fn sources(tags: &[Tag]) -> String {
  vec!["search"; tags.len()].join("/")
}

fn utf16(s: &str) -> usize {
  s.encode_utf16().count()
}
