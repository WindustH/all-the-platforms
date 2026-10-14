//! Episodes: the episode page (show notes, the audio file, counts), its
//! hot comments, and the file `download` saves.

use atp_core::paging::slice;
use atp_core::{Comment, Error, ErrorCode, Media, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::Api;
use crate::refs::{self, Item};
use crate::{parse, shows};

/// The `pageProps` of `/episode/<id>`: `episode` and `comments`.
async fn data(api: &Api, id: &str) -> Result<Value> {
  let props = api.page(&format!("/episode/{id}")).await?;
  if props.str("episode.eid").is_none() {
    return Err(Error::not_found(format!("Xiaoyuzhou has no episode {id}")));
  }
  Ok(props)
}

pub async fn read(api: &Api, id: &str) -> Result<Post> {
  let props = data(api, id).await?;
  Ok(parse::episode(props.at("episode")))
}

/// A show or an episode; a bare id is tried as a show, then as an episode.
pub async fn read_any(api: &Api, arg: &str) -> Result<Post> {
  match refs::resolve(api, arg).await? {
    Item::Show(id) => shows::read(api, &id).await,
    Item::Episode(id) => read(api, &id).await,
    Item::User(_) => Err(Error::input("that is a user; use `user` or `user-posts`")),
    Item::Bare(id) => match shows::read(api, &id).await {
      Err(e) if e.code == ErrorCode::NotFound => read(api, &id).await,
      other => other,
    },
  }
}

/// The episode an argument names (a bare id counts as one).
async fn episode_id(api: &Api, arg: &str) -> Result<String> {
  match refs::resolve(api, arg).await? {
    Item::Episode(id) | Item::Bare(id) => Ok(id),
    _ => Err(Error::input(
      "give an episode (a /episode/ link, or #N after `user-posts`)",
    )),
  }
}

/// The hot comments the episode page shows (about 20, with some replies).
pub async fn comments(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let id = episode_id(api, arg).await?;
  let props = data(api, &id).await?;
  let items: Vec<Comment> = props.list("comments").iter().map(parse::comment).collect();
  Ok(slice(items, page))
}

/// An episode and its audio file, for `download`.
pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let id = episode_id(api, arg).await?;
  let post = read(api, &id).await?;
  if post.media.is_empty() {
    let paid = post.extra.contains_key("pay_type");
    let why = if paid {
      "this is a paid episode; only Xiaoyuzhou's app plays it"
    } else {
      "the episode page gives no audio file"
    };
    return Err(Error::new(ErrorCode::PermissionDenied, why));
  }
  let media = post.media.clone();
  Ok((post, media))
}
