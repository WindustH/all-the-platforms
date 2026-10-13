//! Spaces (live audio): one Space with its hosts, speakers and listener
//! counts, and posts sharing Spaces (X's own Space search no longer answers
//! the web client's `AudioSpaceSearch`).

use social_core::text::from_millis;
use social_core::{Error, Page, PageReq, Post, Query, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::graphql::SPACE_BY_ID;
use crate::refs::{self, space_url};
use crate::{parse, tweets};

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
  post(data.at("data.audioSpace")).ok_or_else(|| Error::not_found(format!("no Space {id}")))
}

/// Posts that share a Space and match `query` (X's `filter:spaces`), newest
/// first, each with the Space's id in `extra.space` for [`space`].
pub async fn search(api: &Api, query: &str, req: &PageReq) -> Result<Page<Post>> {
  let q = Query {
    keyword: format!("{} filter:spaces", query.trim()),
    sort: Some("latest".into()),
    filter: None,
  };
  let mut page = tweets::search(api, &q, req).await?;
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

/// An `audioSpace` object: `metadata` plus `participants`.
fn post(space: &Value) -> Option<Post> {
  let m = space.at("metadata");
  let id = m.str("rest_id")?;
  let time = |k: &str| m.i64(k).and_then(from_millis);
  let names = |role: &str| -> Vec<Value> {
    space
      .list(&format!("participants.{role}"))
      .iter()
      .filter_map(|p| p.str("twitter_screen_name").map(Value::from))
      .collect()
  };
  let mut post = Post {
    kind: "space".into(),
    title: m.str("title"),
    url: Some(space_url(&id)),
    author: parse::user(m.at("creator_results.result")),
    created_at: time("started_at").or_else(|| time("created_at")),
    raw: Some(space.clone()),
    id,
    ..Post::default()
  };
  post.metrics.views = m
    .count("total_live_listeners")
    .or_else(|| m.count("total_replay_watched"));
  let mut put = |key: &str, value: Option<Value>| {
    if let Some(value) = value {
      post.extra.insert(key.into(), value);
    }
  };
  put("state", m.str("state").map(Value::from));
  for key in ["scheduled_start", "ended_at", "updated_at"] {
    put(key, time(key).map(|t| Value::from(t.to_string())));
  }
  put(
    "live_listeners",
    m.count("total_live_listeners").map(Value::from),
  );
  put(
    "replay_views",
    m.count("total_replay_watched").map(Value::from),
  );
  put(
    "replay",
    m.bool("is_space_available_for_replay").map(Value::from),
  );
  put(
    "participants",
    space.count("participants.total").map(Value::from),
  );
  put("media_key", m.str("media_key").map(Value::from));
  for role in ["admins", "speakers"] {
    let list = names(role);
    put(role, (!list.is_empty()).then(|| list.into()));
  }
  Some(post)
}
