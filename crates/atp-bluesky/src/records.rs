//! Writing: everything that goes into your repository as a record
//! (`com.atproto.repo.createRecord` / `deleteRecord`): posts with their
//! links, mentions and hashtags as facets, images, replies and quotes;
//! likes, reposts and follows; and bookmarks (`app.bsky.bookmark.*`).

use std::path::PathBuf;

use atp_core::file::Image;
use atp_core::{Action, Draft, Error, Reply, Result, Value, ValueExt, json};

use crate::api::{self, Api};
use crate::facets::{self, Kind};
use crate::refs::{self, ActorRef, AtUri};
use crate::{identity, posts};

const LIKE: &str = "app.bsky.feed.like";
const REPOST: &str = "app.bsky.feed.repost";
const FOLLOW: &str = "app.bsky.graph.follow";
const MAX_IMAGES: usize = 4;
/// The largest image blob a post takes.
const MAX_IMAGE_BYTES: usize = 1_000_000;

/// `{uri, cid}` of a post view.
fn strong_ref(view: &Value) -> Value {
  json!({"uri": view.str("uri"), "cid": view.str("cid")})
}

/// What a new post holds besides its text.
#[derive(Default)]
struct Compose {
  images: Vec<PathBuf>,
  reply_to: Option<String>,
  quote: Option<String>,
}

/// `post`: topics become hashtags at the end; no title.
pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  if draft.title.is_some() {
    return Err(Error::input(
      "Bluesky posts have no title; leave out --title",
    ));
  }
  let mut text = draft.text.clone();
  for topic in &draft.topics {
    text.push_str(&format!(" #{}", topic.trim_start_matches('#')));
  }
  let compose = Compose {
    images: draft.images.clone(),
    reply_to: draft.reply_to.clone(),
    quote: draft.quote.clone(),
  };
  post(api, text.trim(), &compose).await
}

/// A reply to `post`, or to the reply it answers.
pub async fn comment(api: &Api, post_arg: &str, reply: &Reply) -> Result<Action> {
  let compose = Compose {
    images: reply.images.clone(),
    reply_to: Some(
      reply
        .reply_to
        .clone()
        .unwrap_or_else(|| post_arg.to_owned()),
    ),
    quote: None,
  };
  post(api, reply.text.trim(), &compose).await
}

async fn post(api: &Api, text: &str, compose: &Compose) -> Result<Action> {
  api.did()?;
  let mut record = json!({"$type": refs::POST, "text": text, "createdAt": api::now()});
  let facets = facets(api, text).await;
  if !facets.is_empty() {
    record["facets"] = json!(facets);
  }
  let action = match &compose.reply_to {
    Some(parent) => {
      record["reply"] = reply_refs(api, parent).await?;
      "reply"
    }
    None => "publish",
  };
  if let Some(embed) = embed(api, compose).await? {
    record["embed"] = embed;
  }
  let v = api.create_record(refs::POST, record).await?;
  let uri = v.str("uri").unwrap_or_default();
  let mut done = Action::done(action, &uri).with_id(&uri);
  if let Some(at) = AtUri::parse(&uri) {
    let handle = api
      .ctx
      .extra(crate::api::HANDLE_KEY)
      .filter(|h| !h.is_empty())
      .unwrap_or(at.repo);
    done = done.with_url(refs::post_url(&handle, &at.rkey));
  }
  Ok(done)
}

/// Facets of the links, mentions (of handles that resolve) and hashtags.
async fn facets(api: &Api, text: &str) -> Vec<Value> {
  let mut out = Vec::new();
  for found in facets::scan(text) {
    let feature = match &found.kind {
      Kind::Link(uri) => json!({"$type": "app.bsky.richtext.facet#link", "uri": uri}),
      Kind::Tag(tag) => json!({"$type": "app.bsky.richtext.facet#tag", "tag": tag}),
      Kind::Mention(handle) => match identity::did(api, handle).await {
        Ok(did) => json!({"$type": "app.bsky.richtext.facet#mention", "did": did}),
        Err(_) => continue,
      },
    };
    out.push(json!({
      "index": {"byteStart": found.start, "byteEnd": found.end},
      "features": [feature],
    }));
  }
  out
}

/// `{root, parent}` of a reply to `parent`.
async fn reply_refs(api: &Api, parent: &str) -> Result<Value> {
  let view = posts::view(api, &posts::uri(api, parent).await?).await?;
  let parent = strong_ref(&view);
  let root = match view.at("record.reply.root") {
    root if root.is_object() => root.clone(),
    _ => parent.clone(),
  };
  Ok(json!({"root": root, "parent": parent}))
}

/// Images, a quoted post, or both.
async fn embed(api: &Api, compose: &Compose) -> Result<Option<Value>> {
  let images = if compose.images.is_empty() {
    None
  } else {
    Some(images(api, &compose.images).await?)
  };
  let quote = match &compose.quote {
    Some(arg) => {
      let view = posts::view(api, &posts::uri(api, arg).await?).await?;
      Some(json!({"$type": "app.bsky.embed.record", "record": strong_ref(&view)}))
    }
    None => None,
  };
  Ok(match (images, quote) {
    (Some(media), Some(record)) => Some(json!({
      "$type": "app.bsky.embed.recordWithMedia",
      "record": record,
      "media": media,
    })),
    (media, record) => media.or(record),
  })
}

/// Upload images (four at most, each under 1 MB) as an images embed.
async fn images(api: &Api, paths: &[PathBuf]) -> Result<Value> {
  if paths.len() > MAX_IMAGES {
    return Err(Error::input(format!(
      "a post takes {MAX_IMAGES} images at most"
    )));
  }
  let mut out = Vec::new();
  for path in paths {
    let image = Image::read(path).await?;
    if image.data.len() > MAX_IMAGE_BYTES {
      return Err(
        Error::input(format!(
          "{} is {} bytes; Bluesky takes images up to 1 MB",
          path.display(),
          image.data.len()
        ))
        .with_hint("shrink or recompress it first"),
      );
    }
    let (width, height) = image.size();
    let blob = api.upload_blob(image.data, image.mime).await?;
    out.push(json!({
      "image": blob,
      "alt": "",
      "aspectRatio": {"width": width, "height": height},
    }));
  }
  Ok(json!({"$type": "app.bsky.embed.images", "images": out}))
}

/// Like or repost a post, or take it back (`undo`).
pub async fn mark(api: &Api, arg: &str, repost: bool, undo: bool) -> Result<Action> {
  let (collection, verb, key) = if repost {
    (REPOST, "repost", "viewer.repost")
  } else {
    (LIKE, "like", "viewer.like")
  };
  api.did()?;
  let view = posts::view(api, &posts::uri(api, arg).await?).await?;
  let uri = view.str("uri").unwrap_or_default();
  if undo {
    let record = view
      .str(key)
      .ok_or_else(|| Error::input(format!("you have not done `{verb}` on {uri}")))?;
    api.delete_record(&record).await?;
    return Ok(Action::done(format!("un{verb}"), &uri));
  }
  let record = json!({"$type": collection, "subject": strong_ref(&view), "createdAt": api::now()});
  let v = api.create_record(collection, record).await?;
  Ok(Action::done(verb, &uri).with_id(v.str("uri").unwrap_or_default()))
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let actor = match refs::actor(arg)? {
    ActorRef::Me => return Err(Error::input("you cannot follow yourself")),
    ActorRef::Actor(actor) => actor,
  };
  let profile = api
    .get_auth("app.bsky.actor.getProfile", vec![("actor", actor)])
    .await?;
  let did = profile.str("did").unwrap_or_default();
  if undo {
    let record = profile
      .str("viewer.following")
      .ok_or_else(|| Error::input(format!("you do not follow {did}")))?;
    api.delete_record(&record).await?;
    return Ok(Action::done("unfollow", &did));
  }
  let record = json!({"$type": FOLLOW, "subject": did, "createdAt": api::now()});
  api.create_record(FOLLOW, record).await?;
  Ok(Action::done("follow", &did))
}

/// Save a post to your bookmarks, or take it out.
pub async fn bookmark(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.did()?;
  let view = posts::view(api, &posts::uri(api, arg).await?).await?;
  let uri = view.str("uri").unwrap_or_default();
  if undo {
    api
      .call("app.bsky.bookmark.deleteBookmark", json!({"uri": uri}))
      .await?;
    return Ok(Action::done("unfavorite", &uri));
  }
  api
    .call("app.bsky.bookmark.createBookmark", strong_ref(&view))
    .await?;
  Ok(Action::done("favorite", &uri))
}

/// Delete one of your posts.
pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let uri = posts::uri(api, arg).await?;
  api.delete_record(&uri).await?;
  Ok(Action::done("delete", &uri))
}
