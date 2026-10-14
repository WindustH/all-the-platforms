//! Your shelf (书架), reading progress and the totals they add up to.

use std::collections::HashMap;

use atp_core::paging::slice;
use atp_core::text::from_secs;
use atp_core::{Error, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs;

/// The whole shelf as the web reader syncs it.
async fn sync(api: &Api) -> Result<Value> {
  api.require_login()?;
  let params = [
    ("synckey", "0".to_owned()),
    ("teenmode", "0".to_owned()),
    ("album", "1".to_owned()),
    ("onlyBookid", "0".to_owned()),
  ];
  api.get("/web/shelf/sync", &params).await
}

/// The progress entries of a shelf, by book id.
fn progress_of(v: &Value) -> HashMap<String, &Value> {
  v.list("bookProgress")
    .iter()
    .filter_map(|p| Some((p.str("bookId")?, p)))
    .collect()
}

/// Your shelf, the most recently read first.
pub async fn shelf(
  api: &Api,
  user: Option<&str>,
  folder: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  if user.is_some() || folder.is_some() {
    return Err(Error::input(
      "WeRead shows only your own shelf, whole; leave out the user and --folder",
    ));
  }
  let v = sync(api).await?;
  let progress = progress_of(&v);
  let mut books: Vec<Post> = v
    .list("books")
    .iter()
    .map(|b| parse::shelved(b, b.str("bookId").and_then(|id| progress.get(&id).copied())))
    .collect();
  books.extend(v.list("albums").iter().map(|a| {
    let mut post = parse::shelved(a.at("albumInfo"), None);
    post.kind = "album".into();
    post.title = post.title.or_else(|| a.str("albumInfo.name"));
    post
  }));
  books.sort_by_key(|b| std::cmp::Reverse(b.created_at));
  Ok(slice(books, page))
}

/// How far you are in one book, and how long you have read it.
pub async fn progress(api: &Api, arg: &str) -> Result<Value> {
  let id = refs::book(arg)?;
  api.require_login()?;
  let v = api
    .get("/web/book/getProgress", &[("bookId", id.clone())])
    .await?;
  let b = v.at("book");
  let time = |key: &str| b.i64(key).and_then(from_secs).map(|t| t.to_string());
  Ok(json!({
    "book": id,
    "url": refs::book_url(&id),
    "progress_percent": b.u64("progress"),
    "chapter_index": b.u64("chapterIdx"),
    "reading_seconds": b.u64("recordReadingTime").or_else(|| b.u64("readingTime")),
    "started": time("startReadingTime"),
    "finished": time("finishTime"),
    "updated": time("updateTime"),
    "position_text": b.str("summary"),
  }))
}

/// Totals over your shelf and your notes.
pub async fn stats(api: &Api) -> Result<Value> {
  let v = sync(api).await?;
  let books = v.list("books");
  let progress = progress_of(&v);
  let finished = books
    .iter()
    .filter(|b| b.bool("finishReading") == Some(true))
    .count();
  let started = progress
    .values()
    .filter(|p| p.u64("progress").is_some_and(|n| n > 0))
    .count();
  let seconds: u64 = progress.values().filter_map(|p| p.u64("readingTime")).sum();
  let notes = api.get("/api/user/notebook", &[]).await?;
  Ok(json!({
    "books_on_shelf": books.len(),
    "albums_on_shelf": v.list("albums").len(),
    "finished": finished,
    "started": started,
    "reading_seconds_on_shelf": (seconds > 0).then_some(seconds),
    "books_with_notes": notes.u64("totalBookCount").unwrap_or(notes.list("books").len() as u64),
    "notes": notes.u64("totalNoteCount"),
  }))
}
