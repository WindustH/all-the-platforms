//! Juejin's Markdown (`mark_content`) as plain text: front matter dropped,
//! headings, lists, quotes and tables without their markers, code blocks
//! indented, links as `text (url)`, images as `[image: alt]` (their links
//! are collected for the post's media), emphasis and HTML tags removed.

use std::sync::LazyLock;

use regex::{Captures, Regex};

fn re(pattern: &str) -> Regex {
  Regex::new(pattern).expect("valid pattern")
}

static IMAGE: LazyLock<Regex> =
  LazyLock::new(|| re(r#"!\[([^\]]*)\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)"#));
static LINK: LazyLock<Regex> =
  LazyLock::new(|| re(r#"\[([^\]]+)\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)"#));
static AUTOLINK: LazyLock<Regex> = LazyLock::new(|| re(r"<(https?://[^>\s]+)>"));
static HTML_IMG: LazyLock<Regex> =
  LazyLock::new(|| re(r#"(?i)<img[^>]*\ssrc=["']([^"']+)["'][^>]*>"#));
static BREAK: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)<br\s*/?>"));
static TAG: LazyLock<Regex> = LazyLock::new(|| re(r"</?[a-zA-Z][^>]*>"));
static STRONG: LazyLock<Regex> = LazyLock::new(|| re(r"\*\*(.+?)\*\*|__(.+?)__"));
static EM: LazyLock<Regex> = LazyLock::new(|| re(r"\*([^*\s][^*]*?)\*"));
static EM_UNDER: LazyLock<Regex> = LazyLock::new(|| re(r"(^|[^\w])_([^_\s][^_]*?)_([^\w]|$)"));
static STRIKE: LazyLock<Regex> = LazyLock::new(|| re(r"~~(.+?)~~"));
static HEADING: LazyLock<Regex> = LazyLock::new(|| re(r"^#{1,6}\s+(.*?)\s*#*\s*$"));
static BULLET: LazyLock<Regex> = LazyLock::new(|| re(r"^(\s*)[-*+]\s+(?:\[[ xX]\]\s+)?"));
static RULE: LazyLock<Regex> =
  LazyLock::new(|| re(r"^\s*(?:(?:-\s*){3,}|(?:\*\s*){3,}|(?:_\s*){3,})$"));
static TABLE_RULE: LazyLock<Regex> =
  LazyLock::new(|| re(r"^\s*\|?\s*:?-{2,}:?\s*(?:\|\s*:?-{2,}:?\s*)*\|?\s*$"));

/// The text, and the links of the images in it.
pub fn to_text(markdown: &str) -> (String, Vec<String>) {
  let mut images = Vec::new();
  let mut out: Vec<String> = Vec::new();
  let mut fence: Option<&str> = None;
  for line in without_front_matter(markdown).lines() {
    let marker = line.trim_start();
    if let Some(open) = fence {
      if marker.starts_with(open) {
        fence = None;
      } else {
        out.push(format!("    {line}"));
      }
      continue;
    }
    if marker.starts_with("```") || marker.starts_with("~~~") {
      fence = Some(&marker[..3]);
      continue;
    }
    out.push(block(line, &mut images));
  }
  (tidy(&out), images)
}

/// The text after a leading `---` … `---` block (Juejin's theme settings).
fn without_front_matter(markdown: &str) -> &str {
  let text = markdown.trim_start_matches('\u{feff}');
  let Some(rest) = text.strip_prefix("---") else {
    return text;
  };
  match rest.find("\n---") {
    Some(end) if rest.starts_with('\n') || rest.starts_with("\r\n") => {
      let after = &rest[end + 4..];
      after.split_once('\n').map_or("", |(_, body)| body)
    }
    _ => text,
  }
}

/// One line outside code blocks.
fn block(line: &str, images: &mut Vec<String>) -> String {
  if RULE.is_match(line) && !line.trim().is_empty() {
    return "────────".into();
  }
  if TABLE_RULE.is_match(line) && line.contains('-') && line.contains('|') {
    return String::new();
  }
  if let Some(m) = HEADING.captures(line) {
    return format!("\n{}", inline(&m[1], images));
  }
  let mut text = line.to_owned();
  let trimmed = text.trim_start();
  if let Some(quoted) = trimmed.strip_prefix('>') {
    text = format!("│ {}", quoted.trim_start_matches('>').trim_start());
  } else if trimmed.starts_with('|') {
    text = trimmed
      .trim_matches('|')
      .split('|')
      .map(str::trim)
      .collect::<Vec<_>>()
      .join(" | ");
  } else {
    text = BULLET.replace(&text, "${1}• ").into_owned();
  }
  inline(&text, images)
}

/// Inline markup, leaving `code spans` as they are (without backticks).
fn inline(text: &str, images: &mut Vec<String>) -> String {
  text
    .split('`')
    .enumerate()
    .map(|(i, part)| {
      if i % 2 == 1 {
        part.to_owned()
      } else {
        spans(part, images)
      }
    })
    .collect()
}

fn spans(text: &str, images: &mut Vec<String>) -> String {
  let text = IMAGE.replace_all(text, |c: &Captures| {
    images.push(c[2].to_owned());
    label(&c[1])
  });
  let text = HTML_IMG.replace_all(&text, |c: &Captures| {
    images.push(c[1].to_owned());
    label("")
  });
  let text = LINK.replace_all(&text, |c: &Captures| {
    if c[1] == c[2] {
      c[2].to_owned()
    } else {
      format!("{} ({})", &c[1], &c[2])
    }
  });
  let text = AUTOLINK.replace_all(&text, "$1");
  let text = BREAK.replace_all(&text, "\n");
  let text = TAG.replace_all(&text, "");
  let text = STRONG.replace_all(&text, "$1$2");
  let text = EM.replace_all(&text, "$1");
  let text = EM_UNDER.replace_all(&text, "$1$2$3");
  let text = STRIKE.replace_all(&text, "$1");
  entities(
    &text
      .replace("\\*", "*")
      .replace("\\_", "_")
      .replace("\\#", "#"),
  )
}

fn label(alt: &str) -> String {
  let alt = alt.trim();
  if alt.is_empty() || alt == "image.png" {
    "[image]".into()
  } else {
    format!("[image: {alt}]")
  }
}

fn entities(text: &str) -> String {
  text
    .replace("&nbsp;", " ")
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&amp;", "&")
}

/// Lines joined, trailing spaces cut, runs of blank lines made one.
fn tidy(lines: &[String]) -> String {
  let mut out = String::new();
  let mut blank = 0;
  for line in lines.join("\n").lines() {
    let line = line.trim_end();
    if line.is_empty() {
      blank += 1;
      if blank > 1 {
        continue;
      }
    } else {
      blank = 0;
    }
    out.push_str(line);
    out.push('\n');
  }
  out.trim().to_owned()
}
