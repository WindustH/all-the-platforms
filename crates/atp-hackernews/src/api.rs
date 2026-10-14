//! Hacker News transport: the official Firebase API
//! (`hacker-news.firebaseio.com/v0/<path>.json`: lists of story ids, items,
//! users) and the Algolia search API (`hn.algolia.com/api/v1`: search and
//! whole comment trees in one request). Both are open and need no account.

use std::cell::RefCell;
use std::collections::HashMap;

use atp_core::{Comment, Ctx, Error, PageReq, Result, Value, ValueExt};
use futures_util::{StreamExt, stream};

pub const FIREBASE: &str = "https://hacker-news.firebaseio.com/v0";
pub const ALGOLIA: &str = "https://hn.algolia.com/api/v1";
pub const SITE: &str = "https://news.ycombinator.com";
/// Items fetched at once from Firebase.
const CONCURRENCY: usize = 8;
/// The most hits Algolia returns per page.
pub const HITS_MAX: usize = 100;

pub struct Api {
  pub ctx: Ctx,
  /// Comment threads by item id, built once for the pages of one run.
  pub threads: RefCell<HashMap<u64, Vec<Comment>>>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      threads: RefCell::new(HashMap::new()),
    }
  }

  /// `GET <FIREBASE>/<path>.json`; `null` for an item or user that does not exist.
  pub async fn firebase(&self, path: &str) -> Result<Value> {
    self
      .ctx
      .http
      .get(format!("{FIREBASE}/{path}.json"))
      .no_cookies()
      .value()
      .await
  }

  /// An item, or `not_found`.
  pub async fn item(&self, id: u64) -> Result<Value> {
    let v = self.firebase(&format!("item/{id}")).await?;
    if v.is_null() {
      return Err(Error::not_found(format!("no Hacker News item {id}")));
    }
    Ok(v)
  }

  /// Items in the order of `ids`, a few at a time; missing ones dropped.
  pub async fn items(&self, ids: &[u64]) -> Result<Vec<Value>> {
    let found: Vec<Result<Value>> = stream::iter(ids)
      .map(|id| async move { self.firebase(&format!("item/{id}")).await })
      .buffered(CONCURRENCY)
      .collect()
      .await;
    let mut items = Vec::with_capacity(found.len());
    for item in found {
      let item = item?;
      if !item.is_null() {
        items.push(item);
      }
    }
    Ok(items)
  }

  /// `GET <ALGOLIA>/<path>` with `params`.
  pub async fn algolia(&self, path: &str, params: Vec<(&str, String)>) -> Result<Value> {
    self
      .ctx
      .http
      .get(format!("{ALGOLIA}/{path}"))
      .no_cookies()
      .queries(params)
      .value()
      .await
  }

  /// The whole comment tree under an item (Algolia's `items/<id>`).
  pub async fn tree(&self, id: u64) -> Result<Value> {
    self.algolia(&format!("items/{id}"), Vec::new()).await
  }
}

/// `(page, size)` of an Algolia request (pages count from 0): the cursor
/// carries both (`2:30`), so every page has the size of the first.
pub fn page_of(page: &PageReq) -> (u64, usize) {
  let parsed = page.cursor.as_deref().and_then(|cursor| {
    let (no, size) = cursor.split_once(':')?;
    Some((no.parse().ok()?, size.parse().ok()?))
  });
  parsed.unwrap_or_else(|| (page.number_or(0), page.size_within(HITS_MAX)))
}

/// The ids of a Firebase list (`topstories`, a user's `submitted`, `kids`).
pub fn ids(v: &Value, path: &str) -> Vec<u64> {
  v.list(path).iter().filter_map(Value::as_u64).collect()
}
