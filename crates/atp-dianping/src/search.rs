//! `search`: the www search page (`/search/keyword/<city>/<channel>_<words>`,
//! `/p<n>` for further pages, 15 shops a page). Dianping shows it after a
//! login only; visitors are sent to the check. The city is `DIANPING_CITY`
//! or the session's; `--filter` narrows to a channel.

use atp_core::text::encode_uri_component;
use atp_core::{Error, ErrorCode, Page, PageReq, Post, Query, Result};
use atp_mtsec::guard::Call;

use crate::api::{self, Api, WWW};
use crate::parse;

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let channel = api::channel(query.filter.as_deref())?.unwrap_or("0");
  let number = page.number_or(1);
  let mut url = format!(
    "{WWW}/search/keyword/{}/{channel}_{}",
    api.city(),
    encode_uri_component(&query.keyword)
  );
  if number > 1 {
    url.push_str(&format!("/p{number}"));
  }
  let html = api.html(Call::page(&url, format!("{WWW}/"))).await?;
  let (items, more) = parse::search(&html);
  if items.is_empty() && html.contains("验证中心") {
    return Err(
      Error::new(
        ErrorCode::VerificationRequired,
        "Dianping shows a check instead of search results",
      )
      .with_url(url),
    );
  }
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}
