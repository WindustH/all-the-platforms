//! Search: messages within one room (`room:NAME` in the query), and users.

use social_core::{Error, Page, PageReq, Post, Query, Result, User, ValueExt};

use crate::api::Api;
use crate::{chats, messages, parse};

/// Search one room: `room:NAME` (or `in:NAME`) in the query names it.
pub async fn messages(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let mut room = None;
  let words: Vec<&str> = query
    .keyword
    .split_whitespace()
    .filter(
      |w| match w.strip_prefix("room:").or_else(|| w.strip_prefix("in:")) {
        Some(r) => {
          room = Some(r.to_owned());
          false
        }
        None => true,
      },
    )
    .collect();
  let Some(room) = room else {
    return Err(
      Error::input("Rocket.Chat searches one room at a time")
        .with_hint("add `room:NAME` to the query, e.g. `social rc search 'deploy room:general'`"),
    );
  };
  let rid = chats::room_id(api, &room).await?;
  let offset = page.number_or(0);
  let count = page.size_within(100);
  let v = api
    .get(
      "chat.search",
      &[
        ("roomId", rid),
        ("searchText", words.join(" ")),
        ("offset", offset.to_string()),
        ("count", count.to_string()),
      ],
    )
    .await?;
  let rows = v.list("messages").to_vec();
  let next = (rows.len() >= count).then(|| (offset + rows.len() as u64).to_string());
  Ok(Page::new(messages::posts(api, &rows).await?, next))
}

pub async fn users(api: &Api, term: &str) -> Result<Page<User>> {
  let v = api
    .get(
      "spotlight",
      &[("query", format!("@{}", term.trim_start_matches('@')))],
    )
    .await?;
  let server = api.require()?.server;
  Ok(Page::last(
    v.list("users")
      .iter()
      .map(|u| parse::user(u, &server))
      .collect(),
  ))
}
