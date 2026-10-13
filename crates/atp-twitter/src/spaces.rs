//! Spaces (live audio): one Space with its hosts, speakers and listener
//! counts, and posts sharing Spaces (X's own Space search no longer answers
//! the web client's `AudioSpaceSearch`).

use atp_core::{Error, Page, PageReq, Post, Query, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::graphql::SPACE_BY_ID;
use crate::refs::{self};
use crate::{parse, posts};

pub async fn space(api: &Api, arg: &str) -> Result<Post> {
  api.require_login()?;
  let id = refs::space_id(arg)?;
  let variables = json!({
    "id": id,
    "isMetatagsQuery": false,
    "withReplays": true,
    "withListeners": true,
  });
  let data = api.graphql(&SPACE_BY_ID, variables).await?;
  parse::space(data.at("data.audioSpace")).ok_or_else(|| Error::not_found(format!("no Space {id}")))
}

/// Posts that share a Space and match `query` (X's `filter:spaces`), newest
/// first, each with the Space's id in `extra.space` for [`space`].
pub async fn search(api: &Api, query: &str, req: &PageReq) -> Result<Page<Post>> {
  let q = Query {
    keyword: format!("{} filter:spaces", query.trim()),
    sort: Some("latest".into()),
    filter: None,
  };
  let mut page = posts::search(api, &q, req).await?;
  for post in &mut page.items {
    let found = post
      .extra
      .get("urls")
      .and_then(Value::as_array)
      .into_iter()
      .flatten()
      .filter_map(Value::as_str)
      .find_map(|u| refs::space_id(u).ok().filter(|_| u.contains("/spaces/")));
    if let Some(id) = found {
      post.extra.insert("space".into(), id.into());
    }
  }
  Ok(page)
}
