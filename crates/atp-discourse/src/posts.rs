//! Writing: likes, bookmarks, replies, new topics (with uploaded images)
//! and deleting your posts and topics.

use std::path::PathBuf;

use atp_core::http::Method;
use atp_core::{Action, Draft, Error, Reply, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs::{self, CategoryRef, PostRef};
use crate::topics;

/// Discourse's id of the like action.
const LIKE: u64 = 2;

/// A post argument → the post's JSON (a bare number is a topic, meaning
/// its first post; `comment_ids` reads bare numbers as post ids).
async fn target(api: &Api, arg: &str, comment_ids: bool) -> Result<Value> {
  let post = match refs::post(arg)? {
    PostRef::Topic { id, number: None } if comment_ids => PostRef::Post(id),
    post => post,
  };
  topics::post_json(api, &post).await
}

fn post_id(v: &Value) -> Result<u64> {
  v.u64("id")
    .ok_or_else(|| Error::upstream("the forum answered without a post id"))
}

pub async fn like(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = post_id(&target(api, arg, false).await?)?;
  if undo {
    let path = format!("/post_actions/{id}.json?post_action_type_id={LIKE}");
    api.write(Method::DELETE, &path, None).await?;
    return Ok(Action::done("unlike", id.to_string()));
  }
  let body = json!({"id": id, "post_action_type_id": LIKE});
  api
    .write(Method::POST, "/post_actions.json", Some(body))
    .await?;
  Ok(Action::done("like", id.to_string()))
}

pub async fn bookmark(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let post = target(api, arg, false).await?;
  let id = post_id(&post)?;
  if undo {
    let bookmark = post
      .u64("bookmark_id")
      .ok_or_else(|| Error::input(format!("post {id} is not in your bookmarks")))?;
    api
      .write(Method::DELETE, &format!("/bookmarks/{bookmark}.json"), None)
      .await?;
    return Ok(Action::done("unfavorite", id.to_string()));
  }
  let body = json!({"bookmarkable_id": id, "bookmarkable_type": "Post"});
  api
    .write(Method::POST, "/bookmarks.json", Some(body))
    .await?;
  Ok(Action::done("favorite", id.to_string()))
}

/// A reply in the topic of `post`, answering the post `reply.reply_to`
/// when given (an id `comments` lists).
pub async fn comment(api: &Api, post: &str, reply: &Reply) -> Result<Action> {
  let (topic, number) = match &reply.reply_to {
    Some(answer) => {
      let v = target(api, answer, true).await?;
      (v.u64("topic_id"), v.u64("post_number"))
    }
    None => (Some(topic_id(api, post).await?), None),
  };
  let topic = topic.ok_or_else(|| Error::not_found(format!("no topic for {post}")))?;
  let raw = with_images(api, reply.text.trim(), &reply.images).await?;
  let mut body = json!({"topic_id": topic, "raw": raw});
  if let Some(number) = number {
    body["reply_to_post_number"] = json!(number);
  }
  created(api, body, "reply").await
}

async fn topic_id(api: &Api, arg: &str) -> Result<u64> {
  match refs::post(arg)? {
    PostRef::Topic { id, .. } => Ok(id),
    post => topics::post_json(api, &post)
      .await?
      .u64("topic_id")
      .ok_or_else(|| Error::not_found(format!("no topic for {arg}"))),
  }
}

/// `post`: a new topic (`--title`, `--topic CATEGORY`, `--topic '#tag'`),
/// or with `--reply-to` a reply in that topic.
pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  if let Some(reply_to) = &draft.reply_to {
    let reply = Reply {
      text: draft.text.clone(),
      reply_to: None,
      images: draft.images.clone(),
    };
    return comment(api, reply_to, &reply).await;
  }
  let title = draft
    .title
    .clone()
    .ok_or_else(|| Error::input("a new topic needs --title"))?;
  let raw = with_images(api, draft.text.trim(), &draft.images).await?;
  let mut body = json!({"title": title, "raw": raw});
  let (tags, categories): (Vec<&String>, Vec<&String>) =
    draft.topics.iter().partition(|t| t.starts_with('#'));
  if let Some(category) = categories.first() {
    body["category"] = json!(category_id(api, category).await?);
  }
  if !tags.is_empty() {
    let tags: Vec<&str> = tags.iter().map(|t| t.trim_start_matches('#')).collect();
    body["tags"] = json!(tags);
  }
  created(api, body, "publish").await
}

/// A category argument → its id (slugs are looked up).
async fn category_id(api: &Api, arg: &str) -> Result<u64> {
  let slug = match refs::category(arg)? {
    CategoryRef::Id(id) => return Ok(id),
    CategoryRef::Slug(slug) => slug,
  };
  let all = topics::categories(api).await?;
  all
    .items
    .iter()
    .find(|c| {
      c.raw
        .as_ref()
        .and_then(|v| v.str("slug"))
        .is_some_and(|s| s.eq_ignore_ascii_case(&slug))
    })
    .and_then(|c| c.id.parse().ok())
    .ok_or_else(|| Error::not_found(format!("no category {slug} (see `categories`)")))
}

/// `POST /posts.json`, and the new post's link.
async fn created(api: &Api, body: Value, action: &str) -> Result<Action> {
  let v = api.write(Method::POST, "/posts.json", Some(body)).await?;
  let id = v.str("id").unwrap_or_default();
  let mut done = Action::done(action, &id).with_id(&id);
  if let (Some(topic), Some(slug)) = (v.u64("topic_id"), v.str("topic_slug")) {
    let number = v.u64("post_number").unwrap_or(1);
    done = done.with_url(api.link(&format!("/t/{slug}/{topic}/{number}")));
  }
  Ok(done)
}

/// The text with the images uploaded and placed after it.
async fn with_images(api: &Api, text: &str, images: &[PathBuf]) -> Result<String> {
  let mut raw = text.to_owned();
  for path in images {
    let v = api.upload(path).await?;
    let url = v
      .str("short_url")
      .or_else(|| v.str("url"))
      .ok_or_else(|| Error::upstream("the upload answered without a URL"))?;
    let name = v.str("original_filename").unwrap_or_else(|| "image".into());
    let size = match (v.u64("width"), v.u64("height")) {
      (Some(w), Some(h)) => format!("|{w}x{h}"),
      _ => String::new(),
    };
    raw.push_str(&format!("\n\n![{name}{size}]({url})"));
  }
  Ok(raw.trim().to_owned())
}

/// Delete a post of yours; a topic (or its first post) deletes the topic.
pub async fn delete(api: &Api, arg: &str, comment_ids: bool) -> Result<Action> {
  let post = target(api, arg, comment_ids).await?;
  let id = post_id(&post)?;
  if post.u64("post_number") == Some(1)
    && let Some(topic) = post.u64("topic_id")
  {
    api
      .write(Method::DELETE, &format!("/t/{topic}.json"), None)
      .await?;
    return Ok(Action::done("delete", format!("topic {topic}")));
  }
  api
    .write(Method::DELETE, &format!("/posts/{id}.json"), None)
    .await?;
  Ok(Action::done("delete", id.to_string()))
}
