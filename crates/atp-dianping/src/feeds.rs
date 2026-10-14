//! `feed`: the discovery page (`www.dianping.com/discovery/`, a channel's
//! at `/discovery/a/<id>/`, `p<n>` for further pages), 80 notes a page,
//! open to visitors.

use atp_core::{Error, Page, PageReq, Post, Result};
use atp_mtsec::guard::Call;

use crate::api::{self, Api, WWW};
use crate::parse;

pub async fn feed(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let number = page.number_or(1);
  let mut url = format!("{WWW}/discovery/");
  if let Some(channel) = api::channel(kind)? {
    url.push_str(&format!("a/{channel}/"));
  }
  if number > 1 {
    url.push_str(&format!("p{number}"));
  }
  let html = api
    .html(Call::page(url, format!("{WWW}/discovery/")))
    .await?;
  let dump = parse::assigned(&html, "window.__dx_dump__")
    .ok_or_else(|| Error::upstream("no notes in Dianping's discovery page"))?;
  let (items, more) = parse::feed(&dump);
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}
