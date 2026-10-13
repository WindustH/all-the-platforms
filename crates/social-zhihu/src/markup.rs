//! A small Markdown subset → the HTML Zhihu's editor stores.
//!
//! Blocks: `#` / `##` headings (Zhihu's two levels; deeper ones turn bold),
//! paragraphs (one per line, as the editor writes them), `-` / `1.` lists,
//! `>` quotes, fenced code, `|` tables, `---` rules, an image alone on its
//! line (`![caption](path)`, uploaded and placed there) and a link alone on
//! its line with the title `"card"` (a link card). Inline: `**bold**`,
//! `` `code` `` and `[text](url)`. Everything else is text, HTML-escaped.

use std::path::PathBuf;

/// Rendered HTML with a placeholder where each image goes.
pub struct Markup {
  html: String,
  pub images: Vec<PathBuf>,
  pub text_len: usize,
}

impl Markup {
  /// The HTML with the uploaded images (`html` in order) put in their places.
  pub fn fill(&self, html: &[String]) -> String {
    let mut out = self.html.clone();
    for (i, img) in html.iter().enumerate() {
      out = out.replace(&placeholder(i), img);
    }
    out
  }
}

fn placeholder(i: usize) -> String {
  format!("\u{0}image-{i}\u{0}")
}

enum List {
  None,
  Bullet,
  Numbered,
}

pub fn render(md: &str) -> Markup {
  let mut r = Renderer::default();
  for raw in md.lines() {
    r.line(raw);
  }
  r.finish()
}

/// What is open while lines are read: a list, quote lines, table rows or a
/// fenced code block.
struct Renderer {
  m: Markup,
  list: List,
  quote: Vec<String>,
  table: Vec<Vec<String>>,
  code: Option<(String, Vec<String>)>,
}

impl Default for Renderer {
  fn default() -> Self {
    Self {
      m: Markup {
        html: String::new(),
        images: Vec::new(),
        text_len: 0,
      },
      list: List::None,
      quote: Vec::new(),
      table: Vec::new(),
      code: None,
    }
  }
}

impl Renderer {
  fn line(&mut self, raw: &str) {
    if let Some((lang, lines)) = self.code.as_mut() {
      if raw.trim_start().starts_with("```") {
        let body = escape(&lines.join("\n"));
        self.m.text_len += body.chars().count();
        self.m.html += &format!("<pre lang=\"{}\">{body}</pre>", escape(lang));
        self.code = None;
      } else {
        lines.push(raw.to_owned());
      }
      return;
    }
    let line = raw.trim();
    let bullet = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "));
    let numbered = numbered_item(line);
    self.close_finished(line, bullet.is_some() || numbered.is_some());
    if let Some(lang) = line.strip_prefix("```") {
      self.code = Some((lang.trim().to_owned(), Vec::new()));
    } else if line.is_empty() {
    } else if let Some(item) = bullet {
      self.list_item(List::Bullet, item);
    } else if let Some(item) = numbered {
      self.list_item(List::Numbered, item);
    } else if let Some(text) = line.strip_prefix('>') {
      self.quote.push(inline(text.trim(), &mut self.m.text_len));
    } else if line.starts_with('|') {
      self.table_row(line);
    } else {
      self.block(line);
    }
  }

  /// Close the list, quote or table this line does not continue.
  fn close_finished(&mut self, line: &str, list_item: bool) {
    if !list_item {
      close_list(&mut self.m.html, &mut self.list);
    }
    if !line.starts_with('>') && !self.quote.is_empty() {
      self.m.html += &format!("<blockquote>{}</blockquote>", self.quote.join("<br>"));
      self.quote.clear();
    }
    if !line.starts_with('|') && !self.table.is_empty() {
      self.m.html += &table_html(&self.table);
      self.table.clear();
    }
  }

  fn list_item(&mut self, kind: List, item: &str) {
    open_list(&mut self.m.html, &mut self.list, kind);
    self.m.html += &format!("<li>{}</li>", inline(item, &mut self.m.text_len));
  }

  /// A table row; the `|---|` rule under the header is dropped.
  fn table_row(&mut self, line: &str) {
    let cells: Vec<String> = line
      .trim_matches('|')
      .split('|')
      .map(|c| c.trim().to_owned())
      .collect();
    let rule = cells
      .iter()
      .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':')));
    if !rule {
      self.table.push(cells);
    }
  }

  /// A line standing on its own: rule, image, link card, heading or paragraph.
  fn block(&mut self, line: &str) {
    let m = &mut self.m;
    if matches!(line, "---" | "***" | "___") {
      m.html += "<hr>";
    } else if let Some(path) = image(line) {
      m.html += &placeholder(m.images.len());
      m.images.push(PathBuf::from(path));
    } else if let Some((title, url)) = card(line) {
      m.text_len += title.chars().count();
      m.html += &format!(
        "<a data-draft-node=\"block\" data-draft-type=\"link-card\" href=\"{u}\" data-draft-title=\"{t}\" data-draft-cover=\"\">{t}</a>",
        u = escape(url),
        t = escape(title),
      );
    } else if let Some(h) = line.strip_prefix("# ") {
      m.html += &format!("<h2>{}</h2>", inline(h, &mut m.text_len));
    } else if let Some(h) = line.strip_prefix("## ") {
      m.html += &format!("<h3>{}</h3>", inline(h, &mut m.text_len));
    } else if line.starts_with("###") {
      let h = line.trim_start_matches('#').trim();
      m.html += &format!("<p><b>{}</b></p>", inline(h, &mut m.text_len));
    } else {
      m.html += &format!("<p>{}</p>", inline(line, &mut m.text_len));
    }
  }

  /// Close whatever is still open at the end of the text.
  fn finish(mut self) -> Markup {
    if let Some((lang, lines)) = self.code.take() {
      self.m.html += &format!(
        "<pre lang=\"{}\">{}</pre>",
        escape(&lang),
        escape(&lines.join("\n"))
      );
    }
    close_list(&mut self.m.html, &mut self.list);
    if !self.quote.is_empty() {
      self.m.html += &format!("<blockquote>{}</blockquote>", self.quote.join("<br>"));
    }
    if !self.table.is_empty() {
      self.m.html += &table_html(&self.table);
    }
    self.m
  }
}

fn numbered_item(line: &str) -> Option<&str> {
  let digits = line.bytes().take_while(u8::is_ascii_digit).count();
  (digits > 0)
    .then(|| line[digits..].strip_prefix(". "))
    .flatten()
}

fn open_list(html: &mut String, list: &mut List, want: List) {
  match (&list, &want) {
    (List::Bullet, List::Bullet) | (List::Numbered, List::Numbered) => return,
    _ => close_list(html, list),
  }
  *html += if matches!(want, List::Bullet) {
    "<ul>"
  } else {
    "<ol>"
  };
  *list = want;
}

fn close_list(html: &mut String, list: &mut List) {
  match list {
    List::Bullet => *html += "</ul>",
    List::Numbered => *html += "</ol>",
    List::None => {}
  }
  *list = List::None;
}

fn table_html(rows: &[Vec<String>]) -> String {
  let mut len = 0;
  let mut out = String::from(
    "<table data-draft-node=\"block\" data-draft-type=\"table\" data-size=\"normal\"><tbody>",
  );
  for (i, row) in rows.iter().enumerate() {
    let tag = if i == 0 { "th" } else { "td" };
    out += "<tr>";
    for cell in row {
      out += &format!("<{tag}>{}</{tag}>", inline(cell, &mut len));
    }
    out += "</tr>";
  }
  out + "</tbody></table>"
}

/// `![caption](path)` filling the whole line; the caption is not kept.
fn image(line: &str) -> Option<&str> {
  let rest = line.strip_prefix("![")?;
  let (_, rest) = rest.split_once("](")?;
  let path = rest.strip_suffix(')')?;
  (!path.is_empty() && !path.contains(' ')).then_some(path)
}

/// `[title](url "card")` filling the whole line.
fn card(line: &str) -> Option<(&str, &str)> {
  let rest = line.strip_prefix('[')?;
  let (title, rest) = rest.split_once("](")?;
  let url = rest.strip_suffix(" \"card\")")?;
  Some((title, url))
}

/// Inline markup of one line; adds the visible characters to `len`.
fn inline(text: &str, len: &mut usize) -> String {
  let mut out = String::new();
  let mut rest = text;
  while !rest.is_empty() {
    if let Some(after) = rest.strip_prefix('`')
      && let Some((code, tail)) = after.split_once('`')
    {
      *len += code.chars().count();
      out += &format!("<code>{}</code>", escape(code));
      rest = tail;
    } else if let Some(after) = rest.strip_prefix("**")
      && let Some((bold, tail)) = after.split_once("**")
    {
      out += &format!("<b>{}</b>", inline(bold, len));
      rest = tail;
    } else if let Some(after) = rest.strip_prefix('[')
      && let Some((label, tail)) = after.split_once("](")
      && let Some((url, tail)) = tail.split_once(')')
      && !url.contains(' ')
    {
      out += &format!("<a href=\"{}\">{}</a>", escape(url), inline(label, len));
      rest = tail;
    } else {
      let ch = rest.chars().next().unwrap_or_default();
      *len += 1;
      out += &escape(&ch.to_string());
      rest = &rest[ch.len_utf8()..];
    }
  }
  out
}

fn escape(s: &str) -> String {
  s.replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
}
