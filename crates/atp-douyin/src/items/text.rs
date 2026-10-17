//! The text fields of an item (`text`, `text_extra`, `challenges` ...) as
//! the page derives them from its editor: hashtags as `#name` spans,
//! offsets in UTF-16 units (JavaScript string lengths), an image post's
//! title joined with `。`, an article's topics counted in code points.
//! The edit page fills its editor back from a published work the same way.

use atp_core::{Error, ErrorCode, Result, Value, ValueExt, json};

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

/// The editor's content: its text and the hashtags in it (`start`, `end`
/// in UTF-16 units).
#[derive(Debug, Clone, Default)]
pub struct Editor {
  pub text: String,
  pub spans: Vec<(usize, usize, Tag)>,
}

impl Editor {
  /// The body, then each tag as `#name`, as typed in the editor.
  pub fn typed(body: &str, tags: &[Tag]) -> Self {
    let mut text = body.trim().to_owned();
    let mut spans = Vec::new();
    for tag in tags {
      if !text.is_empty() {
        text.push(' ');
      }
      let start = utf16(&text);
      text.push('#');
      text.push_str(&tag.name);
      spans.push((start, utf16(&text), tag.clone()));
    }
    Self { text, spans }
  }

  pub fn tags(&self) -> Vec<Tag> {
    self.spans.iter().map(|(_, _, tag)| tag.clone()).collect()
  }

  /// The text without its hashtags (and the space before each).
  pub fn body(&self) -> String {
    let units: Vec<u16> = self.text.encode_utf16().collect();
    let (mut out, mut at) = (Vec::new(), 0);
    for (start, end, _) in &self.spans {
      let mut cut = (*start).min(units.len());
      while cut > at && units[cut - 1] == u16::from(b' ') {
        cut -= 1;
      }
      out.extend_from_slice(&units[at.min(cut)..cut]);
      at = (*end).min(units.len());
    }
    out.extend_from_slice(&units[at.min(units.len())..]);
    String::from_utf16_lossy(&out).trim().to_owned()
  }
}

/// The text of a video (`eA` with a title): `title caption`, hashtag
/// offsets moved past the title, `caption_*` offsets within the caption.
pub fn video_text(words: Words) -> Text {
  video_fields(words.title, &Editor::typed(words.body, words.tags))
}

/// [`video_text`] of an editor's content.
pub fn video_fields(title: &str, editor: &Editor) -> Text {
  let title = title.trim();
  let (caption, spans) = (&editor.text, &editor.spans);
  let tags = editor.tags();
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
    caption: caption.clone(),
    title: title.to_owned(),
    extra: Value::Array(extra),
    challenges: challenges(&tags),
    source: sources(&tags),
  }
}

/// The text of an image post (`eA`, then the title joined with `。`).
pub fn image_text(words: Words) -> Text {
  image_fields(words.title, &Editor::typed(words.body, words.tags))
}

/// [`image_text`] of an editor's content.
pub fn image_fields(title: &str, editor: &Editor) -> Text {
  let title = title.trim();
  let (caption, spans) = (&editor.text, &editor.spans);
  let tags = editor.tags();
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
    caption: caption.clone(),
    title: title.to_owned(),
    extra: Value::Array(extra),
    challenges: challenges(&tags),
    source: sources(&tags),
  }
}

/// A published video's title and editor content, as the edit page fills
/// them in: the caption (the description when there is neither a title
/// nor a caption) with its spans moved into it.
pub fn video_editor(aweme: &Value) -> Result<(String, Editor)> {
  let title = aweme.str("item_title").unwrap_or_default();
  let caption = aweme.str("caption").unwrap_or_default();
  let desc = aweme.str("desc").unwrap_or_default();
  let text = if title.is_empty() && caption.is_empty() {
    desc.clone()
  } else {
    caption.clone()
  };
  // Offsets into `desc` move by what precedes the caption in it.
  let shift = utf16(&desc).saturating_sub(utf16(&caption));
  let mut spans = Vec::new();
  for e in aweme.list("text_extra") {
    let kind = e.i64("type").unwrap_or(-1);
    if kind == IMAGE_TITLE || kind == IMAGE_TITLE_DIVIDER {
      continue;
    }
    let (cs, ce) = (offset(e, "caption_start"), offset(e, "caption_end"));
    let (start, end) = if caption.is_empty() {
      (offset(e, "start"), offset(e, "end"))
    } else if cs != 0 || ce != 0 {
      (cs, ce)
    } else {
      let at = |key| offset(e, key).saturating_sub(shift);
      (at("start"), at("end"))
    };
    spans.push((start, end, span_tag(e, kind)?));
  }
  spans.sort_by_key(|(start, _, _)| *start);
  Ok((title, Editor { text, spans }))
}

/// A published image post's title and editor content: its description is
/// the title, `。` and the text (`text_extra` types 7 and 8 mark them).
pub fn image_editor(aweme: &Value) -> Result<(String, Editor)> {
  let desc = aweme.str("desc").unwrap_or_default();
  let units: Vec<u16> = desc.encode_utf16().collect();
  let extra = aweme.list("text_extra");
  let mark = |kind| {
    extra
      .iter()
      .find(|e| e.i64("type") == Some(kind))
      .map(|e| (offset(e, "start"), offset(e, "end")))
  };
  let title_end = mark(IMAGE_TITLE).map_or(0, |(_, end)| end.min(units.len()));
  let body_at = mark(IMAGE_TITLE_DIVIDER).map_or(title_end, |(_, end)| end.min(units.len()));
  let title = String::from_utf16_lossy(&units[..title_end]);
  let text = String::from_utf16_lossy(&units[body_at..]);
  let mut spans = Vec::new();
  for e in extra {
    let kind = e.i64("type").unwrap_or(-1);
    if kind == IMAGE_TITLE || kind == IMAGE_TITLE_DIVIDER {
      continue;
    }
    let start = offset(e, "start").saturating_sub(body_at);
    let end = offset(e, "end").saturating_sub(body_at);
    spans.push((start, end, span_tag(e, kind)?));
  }
  spans.sort_by_key(|(start, _, _)| *start);
  Ok((title, Editor { text, spans }))
}

/// The hashtag of a span; anything else (a mention, an activity) is
/// refused, as ATP cannot write it back.
fn span_tag(e: &Value, kind: i64) -> Result<Tag> {
  if kind != HASHTAG {
    return Err(Error::new(
      ErrorCode::UnsupportedOperation,
      "this work's text @-mentions people or joins an activity, which ATP cannot write \
       back; change it on creator.douyin.com",
    ));
  }
  Ok(Tag {
    name: e.str("hashtag_name").unwrap_or_default(),
    id: e.u64("hashtag_id").unwrap_or(0),
  })
}

fn offset(e: &Value, key: &str) -> usize {
  e.u64(key).unwrap_or(0) as usize
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
