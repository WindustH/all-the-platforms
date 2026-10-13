//! Topics: one topic and its essence (精华) posts.

use atp_core::{Collection, Error, Page, PageReq, Post, Result, ValueExt};

use crate::api::{Api, V4, WWW};
use crate::{api, parse};

pub async fn topic(api: &Api, id: &str) -> Result<Collection> {
  let state = api.page_state(&format!("{WWW}/topic/{id}")).await?;
  let topic = state.at(&format!("entities.topics.{id}"));
  if topic.is_null() {
    return Err(Error::not_found(format!("no topic {id}")));
  }
  Ok(parse::topic(&parse::snake_keys(topic)))
}

/// The topic's essence feed (精华).
pub async fn topic_essence(api: &Api, id: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = api
    .call(
      api
        .get(&format!("{V4}/topics/{id}/feeds/essence"))
        .queries([
          ("offset", page.number_or(0)),
          ("limit", page.size_within(10) as u64),
        ]),
    )
    .await?;
  let items = v
    .list("data")
    .iter()
    .filter_map(|item| parse::post(item.at("target")).or_else(|| parse::post(item)))
    .collect();
  Ok(Page::new(items, api::next_param(&v, "offset")))
}
