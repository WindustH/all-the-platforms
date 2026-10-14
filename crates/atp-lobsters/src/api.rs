//! Lobsters transport: the JSON versions of its pages (`<page>.json`) and,
//! for search, which has none, the HTML page. No account is involved.
//! Requests name ATP as their client: the HTML pages put browsers through
//! a proof-of-work check (Anubis) that a declared tool skips.

use std::cell::RefCell;
use std::collections::HashMap;

use atp_core::http::{TOOL_USER_AGENT, status_error};
use atp_core::{Ctx, Error, Result, Value};

pub const BASE: &str = "https://lobste.rs";
/// Stories per page of every listing.
pub const PAGE_SIZE: usize = 25;

pub struct Api {
  pub ctx: Ctx,
  /// Stories with their comments, by short id, for the pages of one run.
  stories: RefCell<HashMap<String, Value>>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      stories: RefCell::new(HashMap::new()),
    }
  }

  async fn fetch(&self, path: &str, params: Vec<(&str, String)>) -> Result<String> {
    let resp = self
      .ctx
      .http
      .get(format!("{BASE}/{path}"))
      .no_cookies()
      .header("user-agent", TOOL_USER_AGENT)
      .queries(params)
      .send()
      .await?;
    if resp.status.as_u16() == 404 {
      return Err(Error::not_found(format!("nothing at {BASE}/{path}")));
    }
    if !resp.status.is_success() {
      return Err(status_error(resp.status, &resp.text()));
    }
    Ok(resp.text())
  }

  /// `GET <path>` (ending in `.json`).
  pub async fn json(&self, path: &str) -> Result<Value> {
    let text = self.fetch(path, Vec::new()).await?;
    serde_json::from_str(&text)
      .map_err(|e| Error::upstream(format!("Lobsters sent no JSON for {path}: {e}")))
  }

  /// An HTML page.
  pub async fn html(&self, path: &str, params: Vec<(&str, String)>) -> Result<String> {
    self.fetch(path, params).await
  }

  /// A story with its comments (`/s/<id>.json`), kept for the rest of the run.
  pub async fn story(&self, id: &str) -> Result<Value> {
    if let Some(story) = self.stories.borrow().get(id) {
      return Ok(story.clone());
    }
    let story = self.json(&format!("s/{id}.json")).await?;
    self
      .stories
      .borrow_mut()
      .insert(id.to_owned(), story.clone());
    Ok(story)
  }
}

/// The JSON path of page `no` of a listing: `hottest` → `hottest.json`,
/// `page/2.json`; `t/rust` → `t/rust.json`, `t/rust/page/2.json`.
pub fn listing(base: &str, no: u64) -> String {
  match (base, no) {
    (base, 0 | 1) => format!("{base}.json"),
    ("hottest", no) => format!("page/{no}.json"),
    (base, no) => format!("{base}/page/{no}.json"),
  }
}
