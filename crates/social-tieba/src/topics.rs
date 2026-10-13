//! Hot topics (热议) and the threads of a topic.

use social_core::{Page, PageReq, Post, Result, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

/// The hot topic board (热议榜).
pub async fn hot(api: &Api) -> Result<Page<Post>> {
  let v = api.web("/hottopic/browse/topicList", &[]).await?;
  let items = v
    .list("data.bang_topic.topic_list")
    .iter()
    .filter_map(parse::topic)
    .collect();
  Ok(Page::last(items))
}

/// One page of the threads of a hot topic.
pub async fn topic(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let (id, name) = refs::topic(arg)?;
  let pn = req.number_or(1);
  let v = api
    .web(
      "/mo/q/newtopic/topicDetail",
      &[
        ("topic_id", id),
        ("topic_name", name),
        ("is_new", "1".into()),
        ("is_share", "1".into()),
        ("pn", pn.to_string()),
        ("rn", req.size_within(30).to_string()),
        ("offset", "0".into()),
        ("last_id", String::new()),
      ],
    )
    .await?;
  let none = parse::Users::new();
  let items: Vec<Post> = v
    .list("data.relate_thread.thread_list")
    .iter()
    .map(|t| parse::thread(t.at("thread_info"), &none))
    .collect();
  let more = v.str("data.has_more").as_deref() == Some("1") && !items.is_empty();
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}
