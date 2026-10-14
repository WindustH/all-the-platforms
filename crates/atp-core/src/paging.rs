//! Collect items across pages up to a limit.
//!
//! Cursors handed to the user may carry a `!N` suffix: "the page at this
//! platform cursor, minus its first N items". That lets a listing stop in the
//! middle of an upstream page and continue later without skipping or repeating
//! anything, even on platforms whose page size cannot be chosen.
//!
//! A listing runs until it has `--limit` items or the upstream ends it (no
//! further cursor, or a cursor it already gave). It stops early, keeping the
//! cursor, after `--max-pages` upstream pages or after [`MAX_EMPTY_PAGES`]
//! pages in a row with nothing to keep, and says so on stderr.

use std::collections::HashSet;

use jiff::Timestamp;

use crate::error::Result;
use crate::model::{Dated, Keyed, Page};
use crate::output::note;
use crate::platform::PageReq;

/// Stop after this many consecutive pages without new items (but keep the cursor).
const MAX_EMPTY_PAGES: usize = 3;
const SKIP_MARK: char = '!';

fn split_cursor(cursor: Option<String>) -> (Option<String>, usize) {
  let Some(c) = cursor else { return (None, 0) };
  if let Some((base, n)) = c.rsplit_once(SKIP_MARK)
    && let Ok(n) = n.parse()
  {
    return ((!base.is_empty()).then(|| base.to_owned()), n);
  }
  (Some(c), 0)
}

fn join_cursor(cursor: Option<&str>, skip: usize) -> String {
  format!("{}{SKIP_MARK}{skip}", cursor.unwrap_or_default())
}

/// How far one listing goes.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
  /// Items wanted (`--limit`; `usize::MAX` for all of them).
  pub limit: usize,
  /// Upstream pages one run may fetch (`--max-pages`); `None` for no bound.
  pub pages: Option<usize>,
  /// Say on stderr why a listing stopped before its end.
  pub notes: bool,
}

impl Bounds {
  /// `limit` items, as many pages as that takes.
  pub fn items(limit: usize) -> Self {
    Self {
      limit,
      pages: None,
      notes: true,
    }
  }
}

/// Call `fetch` page after page, starting at `cursor`, until `limit` items are
/// collected or the listing ends. Items an upstream repeats (same id) are dropped.
pub async fn collect<T: Keyed>(
  limit: usize,
  cursor: Option<String>,
  fetch: impl AsyncFnMut(PageReq) -> Result<Page<T>>,
) -> Result<Page<T>> {
  collect_within(Bounds::items(limit), cursor, fetch).await
}

/// [`collect`] within `bounds`.
pub async fn collect_within<T: Keyed>(
  bounds: Bounds,
  cursor: Option<String>,
  mut fetch: impl AsyncFnMut(PageReq) -> Result<Page<T>>,
) -> Result<Page<T>> {
  let (mut cursor, mut skip) = split_cursor(cursor);
  let mut taken = Taken::default();
  let (mut pages, mut empty_run) = (0, 0);
  loop {
    if bounds.pages.is_some_and(|max| pages >= max) {
      return Ok(stopped(
        &bounds,
        taken.items,
        cursor,
        format!("{pages} pages (--max-pages)"),
      ));
    }
    let want = bounds.limit - taken.items.len();
    let page = fetch(PageReq {
      cursor: cursor.clone(),
      size: want.saturating_add(skip),
    })
    .await?;
    pages += 1;
    taken.cursors.insert(cursor.clone());
    let next = page.next_cursor.filter(|_| page.has_more);
    let more = next
      .as_ref()
      .is_some_and(|n| !taken.cursors.contains(&Some(n.clone())));
    let before = taken.items.len();
    if let Some(rest) = taken.add(page.items.into_iter().skip(skip), bounds.limit) {
      // More than needed: the next call resumes inside this page.
      let next = join_cursor(cursor.as_deref(), skip + rest);
      return Ok(Page::new(taken.items, Some(next)));
    }
    if !more {
      return Ok(Page::last(taken.items));
    }
    (cursor, skip) = (next, 0);
    empty_run = if taken.items.len() == before {
      empty_run + 1
    } else {
      0
    };
    if taken.items.len() >= bounds.limit {
      return Ok(Page::new(taken.items, cursor));
    }
    if empty_run >= MAX_EMPTY_PAGES {
      let why = format!("{empty_run} pages in a row with nothing to keep");
      return Ok(stopped(&bounds, taken.items, cursor, why));
    }
  }
}

/// What a listing kept so far, and the cursors it went through.
struct Taken<T> {
  items: Vec<T>,
  keys: HashSet<String>,
  cursors: HashSet<Option<String>>,
}

impl<T> Default for Taken<T> {
  fn default() -> Self {
    Self {
      items: Vec::new(),
      keys: HashSet::new(),
      cursors: HashSet::new(),
    }
  }
}

impl<T: Keyed> Taken<T> {
  /// Keep the new items of a page until `limit`; `Some(n)` when the page
  /// holds more than that, `n` being how many of its items were gone through.
  fn add(&mut self, page: impl Iterator<Item = T>, limit: usize) -> Option<usize> {
    for (i, item) in page.enumerate() {
      if self.items.len() >= limit {
        return Some(i);
      }
      if item.key().is_empty() || self.keys.insert(item.key().to_owned()) {
        self.items.push(item);
      }
    }
    None
  }
}

/// A listing cut short: the items so far and the cursor to go on from.
fn stopped<T>(bounds: &Bounds, items: Vec<T>, cursor: Option<String>, why: String) -> Page<T> {
  if bounds.notes {
    note(&format!("stopped after {why}; the cursor continues"));
  }
  Page::new(items, cursor)
}

/// `--since` / `--until` bounds of a listing (inclusive).
#[derive(Debug, Clone, Copy, Default)]
pub struct Window {
  pub since: Option<Timestamp>,
  pub until: Option<Timestamp>,
}

impl Window {
  pub fn is_open(&self) -> bool {
    self.since.is_none() && self.until.is_none()
  }

  /// Undated items are kept: the window cannot judge them.
  pub fn contains(&self, at: Option<Timestamp>) -> bool {
    let Some(at) = at else { return true };
    self.since.is_none_or(|s| at >= s) && self.until.is_none_or(|u| at <= u)
  }

  fn before(&self, at: Option<Timestamp>) -> bool {
    matches!((at, self.since), (Some(at), Some(s)) if at < s)
  }
}

/// [`collect_within`] keeping only items inside `window`. A page whose items
/// are all older than `since` ends the listing, which suits newest-first listings.
pub async fn collect_window<T: Dated + Keyed>(
  bounds: Bounds,
  cursor: Option<String>,
  window: Window,
  mut fetch: impl AsyncFnMut(PageReq) -> Result<Page<T>>,
) -> Result<Page<T>> {
  if window.is_open() {
    return collect_within(bounds, cursor, fetch).await;
  }
  collect_within(bounds, cursor, async |req| {
    let mut page = fetch(req).await?;
    let past = !page.items.is_empty() && page.items.iter().all(|i| window.before(i.date()));
    page.items.retain(|i| window.contains(i.date()));
    if past {
      page.has_more = false;
      page.next_cursor = None;
    }
    Ok(page)
  })
  .await
}

/// One page of a list already held in memory, the cursor being the offset
/// (for upstreams that return everything at once).
pub fn slice<T>(items: Vec<T>, req: &PageReq) -> Page<T> {
  let start = req.number_or(0) as usize;
  let size = req.size.max(1);
  let total = items.len();
  let page: Vec<T> = items.into_iter().skip(start).take(size).collect();
  let next = (start + size < total).then(|| (start + size).to_string());
  Page::new(page, next)
}
