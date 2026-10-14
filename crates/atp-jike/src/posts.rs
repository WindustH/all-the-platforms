//! Posts: reading one (`/1.0/originalPosts/get`, `/1.0/reposts/get`, which
//! answer without a login too), its video, and the writes on posts: like,
//! save (收藏), publish, repost, delete.

use atp_core::{
  Action, Draft, Error, ErrorCode, Media, MediaKind, Post, Result, Value, ValueExt, json,
};

use crate::api::Api;
use crate::refs::{self, Kind, PostRef};
use crate::{parse, uploads};

/// The most pictures one post carries.
const MAX_PICTURES: usize = 9;

async fn get(api: &Api, kind: Kind, id: &str) -> Result<Value> {
  let v = api
    .get(
      &format!("/1.0/{}/get", kind.segment()),
      &[("id", id.to_owned())],
    )
    .await?;
  let data = v.at("data");
  if data.str("id").is_none() {
    return Err(Error::not_found(format!("Jike has no post {id}")));
  }
  Ok(data.clone())
}

/// The post's data and kind; a bare id is tried as a post, then as a repost.
pub async fn fetch(api: &Api, post: &PostRef) -> Result<(Value, Kind)> {
  if let Some(kind) = post.kind {
    return Ok((get(api, kind, &post.id).await?, kind));
  }
  match get(api, Kind::Post, &post.id).await {
    Ok(v) => Ok((v, Kind::Post)),
    Err(e) if matches!(e.code, ErrorCode::NotFound | ErrorCode::InvalidInput) => {
      Ok((get(api, Kind::Repost, &post.id).await?, Kind::Repost))
    }
    Err(e) => Err(e),
  }
}

/// The post an argument names, with its kind settled.
pub async fn resolve(api: &Api, arg: &str) -> Result<(String, Kind)> {
  let post = refs::post(arg)?;
  match post.kind {
    Some(kind) => Ok((post.id, kind)),
    None => Ok((post.id.clone(), fetch(api, &post).await?.1)),
  }
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let post = refs::post(arg)?;
  let (v, _) = fetch(api, &post).await?;
  Ok(parse::post(&v))
}

/// The post and its pictures, or its video's address
/// (`/1.0/mediaMeta/interactive`).
pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let post = read(api, arg).await?;
  let mut media = post.media.clone();
  if post.extra.contains_key("video") {
    let kind = if post.kind == "repost" {
      "REPOST"
    } else {
      "ORIGINAL_POST"
    };
    let params = [("id", post.id.clone()), ("type", kind.to_owned())];
    let v = api
      .post_query("/1.0/mediaMeta/interactive", &params)
      .await?;
    let url = v.str("url").ok_or_else(|| {
      Error::upstream("Jike gave no address for this video (it may want its app's media handshake)")
    })?;
    media.push(Media::new(MediaKind::Video, url));
  }
  Ok((post, media))
}

pub async fn like(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let (id, kind) = resolve(api, arg).await?;
  let verb = if undo { "unlike" } else { "like" };
  api
    .write(
      &format!("/1.0/{}/{verb}", kind.segment()),
      json!({"id": id}),
    )
    .await?;
  Ok(Action::done(verb, id))
}

/// Save to (or remove from) your collection (收藏).
pub async fn favorite(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let (id, kind) = resolve(api, arg).await?;
  let verb = if undo { "uncollect" } else { "collect" };
  api
    .write(
      &format!("/1.0/{}/{verb}", kind.segment()),
      json!({"id": id}),
    )
    .await?;
  Ok(Action::done(
    if undo { "unfavorite" } else { "favorite" },
    id,
  ))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let (id, kind) = resolve(api, arg).await?;
  api
    .write(
      &format!("/1.0/{}/remove", kind.segment()),
      json!({"id": id}),
    )
    .await?;
  Ok(Action::done("delete", id))
}

/// A new post (text, up to 9 pictures, one topic), or with `--quote` a
/// repost with the text as its comment.
pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  api.require_login()?;
  if draft.reply_to.is_some() {
    return Err(Error::input(
      "Jike posts do not reply to posts; use `comment` instead",
    ));
  }
  if let Some(quote) = &draft.quote {
    return repost(api, quote, draft).await;
  }
  if draft.images.len() > MAX_PICTURES {
    return Err(Error::input(format!(
      "Jike takes at most {MAX_PICTURES} pictures"
    )));
  }
  let topic = match draft.topics.as_slice() {
    [] => None,
    [one] => Some(refs::topic(one)?),
    _ => return Err(Error::input("a Jike post goes to one topic at most")),
  };
  let mut keys = Vec::new();
  for path in &draft.images {
    keys.push(uploads::picture(api, path).await?);
  }
  let mut body = json!({"content": draft.text, "pictureKeys": keys, "syncToPersonalUpdate": true});
  if let Some(topic) = topic {
    body["submitToTopic"] = json!(topic);
  }
  let v = api.write("/1.0/originalPosts/create", body).await?;
  Ok(created(&v, "post"))
}

async fn repost(api: &Api, quote: &str, draft: &Draft) -> Result<Action> {
  if !draft.images.is_empty() || !draft.topics.is_empty() {
    return Err(Error::input("a repost carries text only"));
  }
  let (id, kind) = resolve(api, quote).await?;
  let body = json!({
    "content": draft.text,
    "syncComment": false,
    "targetId": id,
    "targetType": kind.api_type(),
  });
  let v = api.write("/1.0/reposts/add", body).await?;
  Ok(created(&v, "repost"))
}

/// The action for a post the answer created.
fn created(v: &Value, action: &str) -> Action {
  let post = parse::post(v.at("data"));
  let mut done = Action::done(action, post.id.clone()).with_id(post.id.clone());
  if let Some(url) = post.url {
    done = done.with_url(url);
  }
  done
}
