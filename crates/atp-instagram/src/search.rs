//! The top search: accounts, hashtags and places matching a word.

use atp_core::{Collection, Page, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse;

async fn topsearch(api: &Api, query: &str) -> Result<Value> {
  api
    .get(
      "web/search/topsearch/",
      &[
        ("query", query.to_owned()),
        ("context", "blended".to_owned()),
      ],
    )
    .await
}

pub async fn users(api: &Api, query: &str) -> Result<Page<User>> {
  let v = topsearch(api, query).await?;
  Ok(Page::last(
    v.list("users")
      .iter()
      .map(|u| parse::user(u.at("user")))
      .collect(),
  ))
}

/// Hashtags, then places.
pub async fn topics(api: &Api, query: &str) -> Result<Page<Collection>> {
  let v = topsearch(api, query).await?;
  let hashtags = v
    .list("hashtags")
    .iter()
    .map(|h| parse::hashtag(h.at("hashtag")));
  let places = v.list("places").iter().map(parse::place);
  Ok(Page::last(hashtags.chain(places).collect()))
}
