//! Groups (小组): a group, its topics, one topic.

use atp_core::{Page, PageReq, Post, Result, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Topic};
use crate::subjects::next_start;

/// The most topics one request lists.
const TOPICS_MAX: usize = 50;

pub async fn group(api: &Api, id: &str) -> Result<Post> {
  let v = api
    .get(&format!("/group/{id}"), &format!("/group/{id}/"), &[])
    .await?;
  Ok(parse::group(&v))
}

/// A group's topics, the newest first (pinned ones first).
pub async fn topics(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = refs::group(arg)?;
  let start = page.number_or(0);
  let params = [
    ("start", start.to_string()),
    ("count", page.size_within(TOPICS_MAX).to_string()),
  ];
  let v = api
    .get(
      &format!("/group/{id}/topics"),
      &format!("/group/{id}/"),
      &params,
    )
    .await?;
  let mut items: Vec<Post> = Vec::new();
  if start == 0 {
    items.extend(v.list("sticky_topics").iter().map(|t| {
      let mut post = parse::topic(t);
      post.extra.insert("sticky".into(), true.into());
      post
    }));
  }
  let rows = v.list("topics");
  items.extend(rows.iter().map(parse::topic));
  Ok(Page::new(items, next_start(&v, start, rows.len())))
}

pub async fn topic(api: &Api, topic: &Topic) -> Result<Post> {
  let id = &topic.id;
  let params: Vec<(&str, String)> = topic
    .spm
    .iter()
    .map(|spm| ("_spm_id", spm.clone()))
    .collect();
  let v = api
    .get(&format!("/group/topic/{id}"), &topic_page(topic), &params)
    .await?;
  Ok(parse::topic(&v))
}

/// The mobile page of a topic, with its `_spm_id`, as the `Referer`.
pub fn topic_page(topic: &Topic) -> String {
  match &topic.spm {
    Some(spm) => format!("/group/topic/{}/?_spm_id={spm}", topic.id),
    None => format!("/group/topic/{}/", topic.id),
  }
}
