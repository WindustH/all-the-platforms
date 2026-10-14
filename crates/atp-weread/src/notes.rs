//! Your notes: the books you annotated, and per book your highlights
//! (划线) and thoughts (想法) in reading order, as a list or as Markdown.

use std::collections::HashMap;
use std::path::Path;

use atp_core::paging::slice;
use atp_core::{
  Data, Downloaded, Error, MediaKind, Page, PageReq, Post, Result, Value, ValueExt, json,
};

use crate::api::Api;
use crate::parse;
use crate::refs;

/// The most pages of thoughts one book is read in.
const MAX_PAGES: usize = 50;

/// The books you highlighted or wrote thoughts in, the latest first.
pub async fn notebooks(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let v = api.get("/api/user/notebook", &[]).await?;
  let mut rows: Vec<&Value> = v.list("books").iter().collect();
  rows.sort_by_key(|b| std::cmp::Reverse(b.i64("sort").unwrap_or(0)));
  let items = rows
    .into_iter()
    .map(|b| {
      let mut post = parse::book(b.at("book"));
      post.created_at = b.i64("sort").and_then(atp_core::text::from_secs);
      for key in ["bookmarkCount", "reviewCount", "noteCount"] {
        if let Some(n) = b.u64(key) {
          post
            .metrics
            .other
            .insert(key.trim_end_matches("Count").into(), n);
        }
      }
      post
    })
    .collect();
  Ok(slice(items, page))
}

/// A book's title, author and chapters, with all your notes in order.
pub struct Notes {
  pub title: String,
  pub author: Option<String>,
  pub url: String,
  pub notes: Vec<Post>,
}

pub async fn notes(api: &Api, arg: &str) -> Result<Notes> {
  let id = refs::book(arg)?;
  api.require_login()?;
  let marks = api
    .get("/web/book/bookmarklist", &[("bookId", id.clone())])
    .await?;
  let chapters: HashMap<u64, (u64, String)> = marks
    .list("chapters")
    .iter()
    .filter_map(|c| {
      let idx = c.u64("chapterIdx").unwrap_or(0);
      Some((
        c.u64("chapterUid")?,
        (idx, c.str("title").unwrap_or_default()),
      ))
    })
    .collect();
  let title_of = |v: &Value| {
    chapters
      .get(&v.u64("chapterUid").unwrap_or(0))
      .map(|c| c.1.clone())
  };
  let mut keyed: Vec<((u64, u64), Post)> = Vec::new();
  for mark in marks.list("updated") {
    keyed.push((
      order(&chapters, mark),
      parse::highlight(mark, title_of(mark).as_deref()),
    ));
  }
  for row in thoughts(api, &id).await? {
    let r = if row.at("review").is_object() {
      row.at("review")
    } else {
      &row
    };
    keyed.push((
      order(&chapters, r),
      parse::thought(&row, title_of(r).as_deref()),
    ));
  }
  keyed.sort_by_key(|(key, _)| *key);
  let book = marks.at("book");
  Ok(Notes {
    title: book.str("title").unwrap_or_else(|| id.clone()),
    author: book.str("author"),
    url: refs::book_url(&id),
    notes: keyed.into_iter().map(|(_, post)| post).collect(),
  })
}

/// Reading order: chapter index, then the place in the chapter. Reviews of
/// the whole book (no chapter) come last.
fn order(chapters: &HashMap<u64, (u64, String)>, v: &Value) -> (u64, u64) {
  let (uid, start) = parse::position(v);
  match chapters.get(&uid) {
    Some((idx, _)) => (*idx, start),
    None if uid == 0 => (u64::MAX, start),
    None => (uid, start),
  }
}

/// All your thoughts in a book, page after page.
async fn thoughts(api: &Api, id: &str) -> Result<Vec<Value>> {
  let mut all = Vec::new();
  let mut synckey = "0".to_owned();
  for _ in 0..MAX_PAGES {
    let params = [
      ("bookId", id.to_owned()),
      ("listType", "11".to_owned()),
      ("mine", "1".to_owned()),
      ("synckey", synckey.clone()),
      ("maxIdx", all.len().to_string()),
      ("count", "100".to_owned()),
    ];
    let v = api.get("/web/review/list", &params).await?;
    let rows = v.list("reviews");
    all.extend(rows.iter().cloned());
    if rows.is_empty() || !v.bool("hasMore").unwrap_or(false) {
      break;
    }
    synckey = v.str("synckey").unwrap_or(synckey);
  }
  Ok(all)
}

/// The notes as Markdown: chapters as headings, highlights and the
/// passages of thoughts as quotes, thoughts as text.
pub fn markdown(notes: &Notes) -> String {
  let mut out = format!("# {}\n\n", notes.title);
  if let Some(author) = &notes.author {
    out.push_str(&format!("{author}\n\n"));
  }
  out.push_str(&format!("<{}>\n", notes.url));
  let mut chapter: Option<String> = None;
  for note in &notes.notes {
    let here = note
      .extra
      .get("chapter")
      .and_then(Value::as_str)
      .map(str::to_owned)
      .or_else(|| (note.kind == "review").then(|| "点评".to_owned()));
    if here.is_some() && here != chapter {
      out.push_str(&format!("\n## {}\n", here.as_deref().unwrap_or_default()));
      chapter = here;
    }
    out.push('\n');
    out.push_str(&note_markdown(note));
  }
  out
}

fn quote(text: &str) -> String {
  text
    .lines()
    .map(|line| format!("> {line}\n"))
    .collect::<String>()
}

fn note_markdown(note: &Post) -> String {
  let text = note.text.as_deref().unwrap_or_default().trim();
  if note.kind == "highlight" {
    return quote(text);
  }
  let mut out = String::new();
  if let Some(passage) = note.extra.get("quote").and_then(Value::as_str) {
    out.push_str(&quote(passage.trim()));
    out.push('\n');
  }
  out.push_str(text);
  out.push('\n');
  out
}

/// Write the Markdown of a book's notes to `path`.
pub fn export(notes: &Notes, path: &Path) -> Result<Data> {
  let text = markdown(notes);
  std::fs::write(path, &text)
    .map_err(|e| Error::input(format!("cannot write {}: {e}", path.display())))?;
  Ok(Data::Downloads(vec![Downloaded {
    kind: MediaKind::File,
    path: path.display().to_string(),
    bytes: text.len() as u64,
  }]))
}

/// The notes of a book as posts, each naming the book.
pub fn posts(notes: Notes) -> Vec<Post> {
  let mut items = notes.notes;
  for post in &mut items {
    post.extra.insert("book_title".into(), json!(notes.title));
  }
  items
}
