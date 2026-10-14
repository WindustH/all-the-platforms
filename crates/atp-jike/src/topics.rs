//! Topics (圈子): the topics a user joined (`/1.0/topics/listSubscribed`),
//! one topic (`/1.0/topics/getDetail`) and the posts of one of its tabs
//! (`/1.0/topics/tabs/<tab>/feed`). Without a login, a topic and its
//! latest posts come from its page on m.okjike.com.

use atp_core::{Collection, Error, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{parse, refs, users};

/// The topics a user joined (`None`: you).
pub async fn joined(api: &Api, user: Option<&str>, req: &PageReq) -> Result<Page<Collection>> {
  api.require_login()?;
  let username = match user {
    Some(user) => refs::user(user)?,
    None => users::whoami(api).await?.id,
  };
  let body = json!({
    "includeRecentPost": "true",
    "filterShortcuts": "true",
    "username": username,
    "sortBy": "DEFAULT",
  });
  let (v, next) = api.list("/1.0/topics/listSubscribed", body, req).await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::topic).collect(),
    next,
  ))
}

async fn detail(api: &Api, id: &str) -> Result<Value> {
  let v = api
    .get("/1.0/topics/getDetail", &[("id", id.to_owned())])
    .await?;
  let data = v.at("data");
  if data.str("id").is_none() {
    return Err(Error::not_found(format!("Jike has no topic {id}")));
  }
  Ok(data.clone())
}

/// The tab to list: the one asked for, else the topic's first feed tab.
fn pick_tab(topic: &Value, wanted: Option<&str>) -> Result<String> {
  let tabs: Vec<String> = topic
    .list("tabs")
    .iter()
    .filter_map(|t| t.str("type"))
    .filter(|t| t != "hybrid")
    .collect();
  match wanted {
    Some(tab) if tabs.iter().any(|t| t == tab) => Ok(tab.to_owned()),
    Some(tab) => Err(Error::input(format!(
      "the topic has no tab `{tab}`; it has: {}",
      tabs.join(", ")
    ))),
    None => tabs
      .into_iter()
      .next()
      .ok_or_else(|| Error::not_found("the topic lists no posts")),
  }
}

/// A topic's posts: one tab's feed, or without a login its page's latest.
pub async fn posts(api: &Api, arg: &str, tab: Option<&str>, req: &PageReq) -> Result<Page<Post>> {
  let id = refs::topic(arg)?;
  if !api.logged_in() {
    let props = api.page(&format!("/topics/{id}")).await?;
    let items = props.list("posts").iter().map(parse::post).collect();
    return Ok(Page::last(items));
  }
  let topic = detail(api, &id).await?;
  let tab = pick_tab(&topic, tab)?;
  let (v, next) = api
    .list(
      &format!("/1.0/topics/tabs/{tab}/feed"),
      json!({"topicId": id}),
      req,
    )
    .await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::post).collect(),
    next,
  ))
}

/// One topic: its details and tabs, or without a login its page's.
pub async fn topic(api: &Api, arg: &str) -> Result<Collection> {
  let id = refs::topic(arg)?;
  let v = if api.logged_in() {
    detail(api, &id).await?
  } else {
    api
      .page(&format!("/topics/{id}"))
      .await?
      .at("topic")
      .clone()
  };
  let mut topic = parse::topic(&v);
  let tabs: Vec<Value> = v
    .list("tabs")
    .iter()
    .map(|t| json!({"tab": t.str("type"), "name": t.str("name")}))
    .collect();
  if !tabs.is_empty() {
    topic.extra.insert("tabs".into(), json!(tabs));
  }
  Ok(topic)
}
