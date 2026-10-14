//! Publishing through media containers: each post (and each item of a
//! carousel) is first a container made from its text and the public URL
//! of its image or video (`POST /me/threads`); once Meta has fetched and
//! processed the media (`status` FINISHED), `threads_publish` turns the
//! container into a post. Also reposts, deletes, hiding replies and the
//! publishing quota.

use std::time::Duration;

use atp_core::{Action, Data, Draft, Error, ErrorCode, Reply, Result, ValueExt, json};
use atp_meta::graph::{Params, fields, params};

use crate::api::Api;
use crate::posts;

/// The most items a carousel holds.
const MAX_ITEMS: usize = 20;
/// Checks of a container's status, three seconds apart.
const STATUS_CHECKS: u32 = 100;

/// A post to publish. Media are public URLs: Meta fetches them itself.
#[derive(Debug, Default)]
pub struct Compose {
  pub text: String,
  pub images: Vec<String>,
  pub videos: Vec<String>,
  pub reply_to: Option<String>,
  pub quote: Option<String>,
  pub topic: Option<String>,
  pub link: Option<String>,
  pub reply_control: Option<String>,
}

/// `post`: the text and the options the shared command has.
pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  if !draft.images.is_empty() {
    return Err(
      Error::unsupported("post -i FILE")
        .with_hint("Threads fetches media from public URLs: `atp threads thread TEXT --image URL`"),
    );
  }
  if draft.title.is_some() {
    return Err(Error::input(
      "Threads posts have no title; leave out --title",
    ));
  }
  let compose = Compose {
    text: draft.text.clone(),
    reply_to: draft.reply_to.clone(),
    quote: draft.quote.clone(),
    topic: draft
      .topics
      .first()
      .map(|t| t.trim_start_matches('#').to_owned()),
    ..Compose::default()
  };
  create_post(api, &compose).await
}

/// A reply to `post`, or to the reply it answers.
pub async fn comment(api: &Api, post: &str, reply: &Reply) -> Result<Action> {
  let compose = Compose {
    text: reply.text.clone(),
    reply_to: Some(reply.reply_to.clone().unwrap_or_else(|| post.to_owned())),
    ..Compose::default()
  };
  create_post(api, &compose).await
}

pub async fn create_post(api: &Api, compose: &Compose) -> Result<Action> {
  let items: Vec<(&str, &str)> = compose
    .images
    .iter()
    .map(|url| ("IMAGE", url.as_str()))
    .chain(compose.videos.iter().map(|url| ("VIDEO", url.as_str())))
    .collect();
  if items.len() > MAX_ITEMS {
    return Err(Error::input(format!(
      "a carousel takes {MAX_ITEMS} items at most"
    )));
  }
  let mut form = options(api, compose).await?;
  let container = match items.as_slice() {
    [] => {
      form.extend(params(&[("media_type", "TEXT")]));
      create(api, form).await?
    }
    [(kind, url)] => {
      form.extend(item(kind, url));
      create(api, form).await?
    }
    many => {
      let mut children = Vec::new();
      for (kind, url) in many {
        let mut child = item(kind, url);
        child.extend(params(&[("is_carousel_item", "true")]));
        let id = create(api, child).await?;
        ready(api, &id).await?;
        children.push(id);
      }
      let children = children.join(",");
      form.extend(params(&[
        ("media_type", "CAROUSEL"),
        ("children", &children),
      ]));
      create(api, form).await?
    }
  };
  ready(api, &container).await?;
  let action = if compose.reply_to.is_some() {
    "reply"
  } else {
    "publish"
  };
  publish_container(api, &container, action).await
}

/// The container fields of one image or video.
fn item(kind: &str, url: &str) -> Params {
  let key = if kind == "VIDEO" {
    "video_url"
  } else {
    "image_url"
  };
  params(&[("media_type", kind), (key, url)])
}

/// Text, reply, quote, topic, link and who may reply.
async fn options(api: &Api, compose: &Compose) -> Result<Params> {
  let mut form = params(&[("text", compose.text.trim())]);
  if let Some(reply_to) = &compose.reply_to {
    form.extend(params(&[("reply_to_id", &posts::id(api, reply_to).await?)]));
  }
  if let Some(quote) = &compose.quote {
    form.extend(params(&[("quote_post_id", &posts::id(api, quote).await?)]));
  }
  for (key, value) in [
    ("topic_tag", &compose.topic),
    ("link_attachment", &compose.link),
    ("reply_control", &compose.reply_control),
  ] {
    if let Some(value) = value {
      form.extend(params(&[(key, value)]));
    }
  }
  Ok(form)
}

/// `POST /me/threads`: a container's id.
async fn create(api: &Api, form: Params) -> Result<String> {
  api.pause().await;
  let v = api.graph().await?.post("me/threads", &form).await?;
  v.str("id")
    .ok_or_else(|| Error::upstream("Threads made no container"))
}

/// Wait until Meta has processed a container (five minutes at most).
async fn ready(api: &Api, container: &str) -> Result<()> {
  for _ in 0..STATUS_CHECKS {
    let v = api
      .graph()
      .await?
      .get(container, &fields("status,error_message"))
      .await?;
    match v.str("status").as_deref() {
      Some("FINISHED" | "PUBLISHED") | None => return Ok(()),
      Some("ERROR" | "EXPIRED") => {
        let why = v
          .str("error_message")
          .unwrap_or_else(|| "no reason given".into());
        return Err(
          Error::upstream(format!("Threads could not use the media: {why}"))
            .with_hint("the URLs must be public and point at a JPEG/PNG image or an MP4/MOV video"),
        );
      }
      _ => tokio::time::sleep(Duration::from_secs(3)).await,
    }
  }
  Err(Error::upstream(
    "the media were still processing after five minutes; try again",
  ))
}

/// `threads_publish`: the container becomes a post.
async fn publish_container(api: &Api, container: &str, action: &str) -> Result<Action> {
  let graph = api.graph().await?;
  let v = graph
    .post("me/threads_publish", &params(&[("creation_id", container)]))
    .await?;
  let id = v
    .str("id")
    .ok_or_else(|| Error::upstream("Threads published nothing"))?;
  let mut done = Action::done(action, &id).with_id(&id);
  if let Ok(v) = graph.get(&id, &fields("permalink")).await
    && let Some(url) = v.str("permalink")
  {
    done = done.with_url(url);
  }
  Ok(done)
}

pub async fn repost(api: &Api, arg: &str) -> Result<Action> {
  let id = posts::id(api, arg).await?;
  api.pause().await;
  let v = api
    .graph()
    .await?
    .post(&format!("{id}/repost"), &Vec::new())
    .await?;
  Ok(Action::done("repost", &id).with_id(v.str("id").unwrap_or_default()))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let id = posts::id(api, arg).await?;
  api.pause().await;
  api.graph().await?.delete(&id).await.map_err(|e| {
    if e.code == ErrorCode::PermissionDenied {
      e.with_hint("deleting needs the threads_delete permission on your app and token")
    } else {
      e
    }
  })?;
  Ok(Action::done("delete", &id))
}

/// Hide a reply to one of your posts, or show it again.
pub async fn hide(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = posts::id(api, arg).await?;
  let hide = if undo { "false" } else { "true" };
  api.pause().await;
  api
    .graph()
    .await?
    .post(&format!("{id}/manage_reply"), &params(&[("hide", hide)]))
    .await?;
  Ok(Action::done(if undo { "unhide" } else { "hide" }, &id))
}

/// How much of the daily publishing quota is used (250 posts, 1000
/// replies a day).
pub async fn quota(api: &Api) -> Result<Data> {
  let query = fields("quota_usage,config,reply_quota_usage,reply_config");
  let v = api
    .graph()
    .await?
    .get("me/threads_publishing_limit", &query)
    .await?;
  let row = v.at("data.0");
  Ok(Data::Value(json!({
    "posts_used": row.u64("quota_usage"),
    "posts_allowed": row.u64("config.quota_total"),
    "replies_used": row.u64("reply_quota_usage"),
    "replies_allowed": row.u64("reply_config.quota_total"),
    "window_seconds": row.u64("config.quota_duration"),
  })))
}
