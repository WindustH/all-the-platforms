//! Search: the result page's embedded cards (`pd=information`). Toutiao
//! answers suspicious clients with a slide captcha, which is reported, not
//! solved.

use atp_core::{Error, ErrorCode, Page, PageReq, Post, Query, Result};

use crate::api::{Api, SEARCH};
use crate::parse::search::{cards, is_captcha};

pub async fn search(api: &Api, query: &Query, req: &PageReq) -> Result<Page<Post>> {
  let page = req.number_or(0);
  let resp = api
    .ctx
    .http
    .get(format!("{SEARCH}/search"))
    .query("keyword", &query.keyword)
    .query("pd", query.filter.as_deref().unwrap_or("information"))
    .query("dvpf", "pc")
    .query("source", if page == 0 { "input" } else { "pagination" })
    .query("page_num", page)
    .header("accept", "text/html,application/xhtml+xml")
    .send()
    .await?;
  let html = resp.text();
  if is_captcha(&html) || resp.url.contains("verify") {
    return Err(
      Error::new(ErrorCode::VerificationRequired, "Toutiao asks for a slide captcha before searching")
        .with_hint("pass it once at so.toutiao.com in a browser, then `atp toutiao login --browser`; or retry later"),
    );
  }
  let posts = cards(&html);
  let next = (!posts.is_empty()).then(|| (page + 1).to_string());
  Ok(Page::new(posts, next))
}

/// `search --filter` values (the search page's `pd`).
pub const FILTERS: &[&str] = &["information", "video", "weitoutiao", "synthesis"];
