//! Editing a post as the composer does: the text through
//! `PUT /posts/{id}.json` (`raw`, with the text it replaces as
//! `original_text`, so an edit made meanwhile is a conflict instead of
//! being overwritten), and a topic's title, tags and category through
//! `PUT /t/-/{topic}.json` (with `original_title` and `original_tags` for
//! the same check). Forums limit how long a post stays editable
//! (`post_edit_time_limit`, a day for new users; 30 days from trust level
//! 2 by default), and locked or archived topics cannot change.

use atp_core::cli::Change;
use atp_core::http::Method;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::posts;
use crate::refs;
use crate::topics;

/// The topic fields to change, as `PUT /t/-/{topic}.json` takes them.
#[derive(Default)]
struct TopicChange {
  title: Option<String>,
  /// Tag names replacing the topic's own (empty removes them).
  tags: Option<Vec<String>>,
  category: Option<u64>,
}

/// Change one of your posts: its text, and for a topic's first post the
/// topic's title, tags and category.
pub async fn edit(api: &Api, arg: &str, edit: &Edit) -> Result<Action> {
  api.require()?;
  let post = topics::post_json(api, &refs::post(arg)?).await?;
  let id = post.u64("id").unwrap_or_default();
  let topic_id = post.u64("topic_id").unwrap_or_default();
  editable(&post, id)?;
  let wanted = topic_change(api, edit).await?;
  let is_topic_change =
    wanted.title.is_some() || wanted.tags.is_some() || wanted.category.is_some();
  if is_topic_change && post.u64("post_number") != Some(1) {
    return Err(Error::input(
      "--title and --topic change a topic: give the topic or its first post",
    ));
  }
  let topic = if is_topic_change {
    Some(topic_json(api, topic_id).await?)
  } else {
    None
  };
  let mut change = Change::new(format!("Discourse post {id} (topic {topic_id}):"));
  let raw = post.str("raw").unwrap_or_default();
  if let Some(text) = &edit.text {
    change.field("text", &raw, text);
  }
  let topic_body = match &topic {
    Some(topic) => Some(topic_body(api, topic, &wanted, &mut change).await?),
    None => None,
  };
  change.confirm(edit.yes, "change this post on Discourse")?;
  if let Some(text) = &edit.text {
    let body = json!({ "post": { "raw": text, "original_text": raw } });
    let path = format!("/posts/{id}.json");
    api.write(Method::PUT, &path, Some(body)).await?;
  }
  if let Some(body) = topic_body {
    let path = format!("/t/-/{topic_id}.json");
    api.write(Method::PUT, &path, Some(body)).await?;
  }
  let slug = post.str("topic_slug").unwrap_or_else(|| "-".into());
  let number = post.u64("post_number").unwrap_or(1);
  let url = api.link(&format!("/t/{slug}/{topic_id}/{number}"));
  Ok(
    Action::done("edit", id.to_string())
      .with_id(id.to_string())
      .with_url(url),
  )
}

/// Refuse before asking when the forum says the post cannot change.
fn editable(post: &Value, id: u64) -> Result<()> {
  if post.bool("can_edit") != Some(false) {
    return Ok(());
  }
  let text = if post.bool("yours") == Some(false) {
    format!("post {id} is not yours")
  } else {
    format!(
      "Discourse no longer lets you edit post {id}: it is past the forum's edit time limit, \
       or its topic is locked or archived"
    )
  };
  Err(Error::new(ErrorCode::PermissionDenied, text))
}

/// `--topic`: names with `#` replace the tags (`--no-topics` removes
/// them), the first without `#` is the category to move the topic to,
/// as with `post`.
async fn topic_change(api: &Api, edit: &Edit) -> Result<TopicChange> {
  let mut wanted = TopicChange {
    title: edit.title.as_deref().map(str::trim).map(str::to_owned),
    ..TopicChange::default()
  };
  if wanted.title.as_deref() == Some("") {
    return Err(Error::input("a topic needs a title"));
  }
  let Some(topics) = &edit.topics else {
    return Ok(wanted);
  };
  let (tags, categories): (Vec<&String>, Vec<&String>) =
    topics.iter().partition(|t| t.starts_with('#'));
  if topics.is_empty() || !tags.is_empty() {
    let names = tags.iter().map(|t| t.trim_start_matches('#').to_owned());
    wanted.tags = Some(names.collect());
  }
  if let Some(category) = categories.first() {
    wanted.category = Some(posts::category_id(api, category).await?);
  }
  Ok(wanted)
}

async fn topic_json(api: &Api, topic_id: u64) -> Result<Value> {
  let topic = api.get(&format!("/t/{topic_id}.json"), Vec::new()).await?;
  if topic.bool("details.can_edit") == Some(false) {
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("Discourse does not let you change topic {topic_id}"),
    ));
  }
  Ok(topic)
}

/// The body of the topic update, its rows added to `change`. The
/// `original_*` fields are the topic as loaded; tags go as objects
/// (`{name}`) where the forum gives them so, else as names.
async fn topic_body(
  api: &Api,
  topic: &Value,
  wanted: &TopicChange,
  change: &mut Change,
) -> Result<Value> {
  let title = topic.str("title").unwrap_or_default();
  let mut body = json!({ "original_title": title });
  if let Some(new) = &wanted.title {
    change.field("title", &title, new);
    body["title"] = json!(new);
  }
  if let Some(new) = &wanted.tags {
    let Some(old) = topic.get("tags").and_then(Value::as_array) else {
      return Err(Error::unsupported(
        "edit --topic '#tag' (the forum has no tags)",
      ));
    };
    let names: Vec<String> = old.iter().filter_map(tag_name).collect();
    change.field("tags", &names.join(", "), &new.join(", "));
    body["tags"] = if tag_objects(api, old).await {
      json!(
        new
          .iter()
          .map(|name| json!({ "name": name }))
          .collect::<Vec<_>>()
      )
    } else {
      json!(new)
    };
    if !old.is_empty() {
      body["original_tags"] = json!(old);
    }
  }
  if let Some(category) = wanted.category {
    let old = topic.u64("category_id").unwrap_or_default();
    change.field("category", &old.to_string(), &category.to_string());
    body["category_id"] = json!(category);
  }
  Ok(body)
}

fn tag_name(tag: &Value) -> Option<String> {
  tag.as_str().map(str::to_owned).or_else(|| tag.str("name"))
}

/// Whether the forum takes tags as objects (`{id, name}`: Discourse
/// 2026.01 on, required from 2026.07) rather than names: as the topic
/// lists its own, or for a topic without tags by the forum's version
/// (`/about.json`; year-numbered versions take objects).
async fn tag_objects(api: &Api, tags: &[Value]) -> bool {
  if let Some(first) = tags.first() {
    return first.is_object();
  }
  let Ok(about) = api.get("/about.json", Vec::new()).await else {
    return true;
  };
  let version = about.str("about.version").unwrap_or_default();
  let major: u32 = version
    .split(|c: char| !c.is_ascii_digit())
    .next()
    .and_then(|n| n.parse().ok())
    .unwrap_or(0);
  major == 0 || major >= 2026
}
