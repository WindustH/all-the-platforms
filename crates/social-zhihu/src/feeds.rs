//! Feeds: the hot list, recommendations, the follow feed and a user's
//! activities (动态).

use social_core::{Page, PageReq, Post, Result, ValueExt};

use crate::api::{Api, MOBILE, V3};
use crate::{api, parse};

/// Activity of the people and topics you follow; the cursor is the query of `paging.next`.
pub async fn follow(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  moments(api, &format!("{V3}/moments"), page).await
}

/// What a user did lately: answered, posted, voted, followed ...
pub async fn activities(api: &Api, token: &str, page: &PageReq) -> Result<Page<Post>> {
  moments(api, &format!("{V3}/moments/{token}/activities"), page).await
}

async fn moments(api: &Api, base: &str, page: &PageReq) -> Result<Page<Post>> {
  let req = match &page.cursor {
    Some(query) => api.get(&format!("{base}?{query}")),
    None => api.get(base).queries([
      ("limit", page.size_within(20).to_string()),
      ("desktop", "true".to_owned()),
    ]),
  };
  let v = api.call(req).await?;
  let items = v.list("data").iter().filter_map(parse::activity).collect();
  Ok(Page::new(items, api::next_query(&v)))
}

/// The hot list (热榜). The web endpoint needs a login; the mobile one serves the same list anonymously.
/// Both ignore the old category names (`science`, `digital` ...): every one gives the full list.
pub async fn hot(api: &Api) -> Result<Page<Post>> {
  let req = if api.ctx.http.has_cookie("z_c0") {
    api
      .get(&format!("{V3}/feed/topstory/hot-lists/total"))
      .query("desktop", "true")
  } else {
    api.get(&format!("{MOBILE}/topstory/hot-lists/total"))
  };
  let v = api.call_public(req.query("limit", 50)).await?;
  Ok(Page::last(
    v.list("data").iter().filter_map(parse::hot_item).collect(),
  ))
}

/// The recommendation feed (推荐); the cursor is the query of `paging.next`.
pub async fn feed(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let base = format!("{V3}/feed/topstory/recommend");
  let req = match &page.cursor {
    Some(query) => api.get(&format!("{base}?{query}")),
    None => api.get(&base).queries([
      ("desktop", "true".to_owned()),
      ("action", "down".to_owned()),
      ("page_number", "1".to_owned()),
      ("limit", page.size_within(10).to_string()),
    ]),
  };
  let v = api.call(req).await?;
  let items = v
    .list("data")
    .iter()
    .filter_map(|item| parse::post(item.at("target")))
    .collect();
  Ok(Page::new(items, api::next_query(&v)))
}
