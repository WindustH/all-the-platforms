//! Sections of a platform's `GUIDE.md`: `## Logging in`, and the notes on
//! single commands, which are the `### name, other name` sections under
//! `## Commands`. Headings inside fenced code blocks do not count.

/// The heading that holds the notes on single commands.
pub const COMMANDS: &str = "Commands";
/// The heading that explains logging in.
pub const LOGIN: &str = "Logging in";

struct Heading<'a> {
  level: usize,
  title: &'a str,
  /// Where the heading line starts and where the line after it starts.
  start: usize,
  body: usize,
}

fn headings(md: &str) -> Vec<Heading<'_>> {
  let mut out = Vec::new();
  let mut fenced = false;
  let mut at = 0;
  for line in md.split_inclusive('\n') {
    let start = at;
    at += line.len();
    if line.starts_with("```") {
      fenced = !fenced;
      continue;
    }
    let level = line.bytes().take_while(|b| *b == b'#').count();
    if fenced || level == 0 || line.as_bytes().get(level) != Some(&b' ') {
      continue;
    }
    out.push(Heading {
      level,
      title: line[level..].trim(),
      start,
      body: at,
    });
  }
  out
}

/// The text from heading `i` up to the next heading of its level or above.
fn body<'a>(md: &'a str, all: &[Heading<'_>], i: usize) -> &'a str {
  let level = all[i].level;
  let end = all[i + 1..]
    .iter()
    .find(|h| h.level <= level)
    .map_or(md.len(), |h| h.start);
  md[all[i].body..end].trim_matches('\n')
}

/// The body of the `## title` section, without its heading.
pub fn section<'a>(md: &'a str, title: &str) -> Option<&'a str> {
  let all = headings(md);
  let i = all
    .iter()
    .position(|h| h.level == 2 && h.title.eq_ignore_ascii_case(title))?;
  Some(body(md, &all, i))
}

/// The command paths a `### ` heading names: `like, unlike` →
/// [`like`, `unlike`]; backticks and extra spaces are ignored.
fn names(title: &str) -> Vec<String> {
  title
    .split(',')
    .map(|n| {
      n.replace('`', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
    })
    .filter(|n| !n.is_empty())
    .collect()
}

/// The notes on the command at `path` (`["playlist", "create"]`): every
/// section under `## Commands` naming it or a group it is part of
/// (`playlist`), as (heading, body) in the order of the file.
pub fn notes<'a>(md: &'a str, path: &[&str]) -> Vec<(&'a str, &'a str)> {
  let all = headings(md);
  let Some(top) = all
    .iter()
    .position(|h| h.level == 2 && h.title.eq_ignore_ascii_case(COMMANDS))
  else {
    return Vec::new();
  };
  let wanted: Vec<String> = (1..=path.len()).map(|n| path[..n].join(" ")).collect();
  let mut out = Vec::new();
  for (i, h) in all.iter().enumerate().skip(top + 1) {
    if h.level <= 2 {
      break;
    }
    if h.level == 3 && names(h.title).iter().any(|n| wanted.contains(n)) {
      out.push((h.title, body(md, &all, i)));
    }
  }
  out
}

/// `md` with `summary` right under its `## Logging in` heading, or in a
/// section of its own before everything when there is none.
pub fn with_login(md: &str, summary: &str) -> String {
  let all = headings(md);
  match all
    .iter()
    .find(|h| h.level == 2 && h.title.eq_ignore_ascii_case(LOGIN))
  {
    Some(h) => format!(
      "{}\n{summary}\n{}",
      &md[..h.body],
      md[h.body..].trim_start_matches('\n')
    ),
    None => format!("## {LOGIN}\n\n{summary}\n{md}"),
  }
}

/// `### ` headings one level up, for a section shown as a page of its own.
pub fn promote(md: &str) -> String {
  let mut fenced = false;
  let mut out = String::new();
  for line in md.lines() {
    if line.starts_with("```") {
      fenced = !fenced;
    }
    match line.strip_prefix("### ") {
      Some(rest) if !fenced => out.push_str(&format!("## {rest}")),
      _ => out.push_str(line),
    }
    out.push('\n');
  }
  out
}
