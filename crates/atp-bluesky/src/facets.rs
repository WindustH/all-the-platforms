//! Rich text of a new post: the links, mentions and hashtags in its text,
//! with the byte ranges (UTF-8) facets point at. Mentions still need
//! their handle turned into a DID.

/// What a stretch of the text is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
  Link(String),
  /// A handle, without the `@`.
  Mention(String),
  /// A hashtag, without the `#`.
  Tag(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
  pub start: usize,
  pub end: usize,
  pub kind: Kind,
}

const LEADING: &[char] = &['(', '[', '<', '"', '\'', '“', '‘'];
const TRAILING: &[char] = &[
  '.', ',', ';', ':', '!', '?', ')', ']', '>', '"', '\'', '”', '’', '。', '，', '！', '？',
];

/// Every link, mention and hashtag of `text`.
pub fn scan(text: &str) -> Vec<Found> {
  let mut found = Vec::new();
  let mut at = 0;
  for word in text.split_inclusive(char::is_whitespace) {
    let start = at;
    at += word.len();
    let trimmed = word.trim_end();
    let lead = trimmed.len() - trimmed.trim_start_matches(LEADING).len();
    let token = trimmed[lead..].trim_end_matches(TRAILING);
    if let Some(kind) = classify(token) {
      found.push(Found {
        start: start + lead,
        end: start + lead + token.len(),
        kind,
      });
    }
  }
  found
}

fn classify(token: &str) -> Option<Kind> {
  if token.starts_with("https://") || token.starts_with("http://") {
    return (token.len() > "https://".len()).then(|| Kind::Link(token.to_owned()));
  }
  if let Some(handle) = token.strip_prefix('@') {
    let valid = handle.contains('.')
      && !handle.starts_with('.')
      && handle
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    return valid.then(|| Kind::Mention(handle.to_ascii_lowercase()));
  }
  if let Some(tag) = token.strip_prefix('#') {
    let valid = !tag.is_empty()
      && tag.chars().count() <= 64
      && !tag.chars().all(|c| c.is_ascii_digit())
      && !tag.contains('#');
    return valid.then(|| Kind::Tag(tag.to_owned()));
  }
  None
}
