//! Reading: transcripts and caption tracks, playlists, related videos,
//! hashtags, search suggestions and live chat.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result, json};

use crate::api::Api;
use crate::{captions, library, live, refs, search, videos};

pub async fn transcript(api: &Api, video: &str, lang: Option<&str>) -> Result<Data> {
  let video = api.ctx.post_ref(video)?;
  Ok(Data::Transcript(
    captions::transcript(api, &video, lang).await?,
  ))
}

pub async fn captions(api: &Api, video: &str) -> Result<Data> {
  Ok(Data::Value(
    captions::tracks(api, &api.ctx.post_ref(video)?).await?,
  ))
}

pub async fn playlist(api: &Api, playlist: &str, page: &PageArgs) -> Result<Data> {
  let id = refs::playlist(playlist)?;
  let posts = page.collect_dated(async |r| library::playlist(api, &id, &r).await);
  Ok(Data::Posts(posts.await?))
}

pub async fn related(api: &Api, video: &str, page: &PageArgs) -> Result<Data> {
  let video = api.ctx.post_ref(video)?;
  let posts = page.collect_dated(async |r| videos::related(api, &video, &r).await);
  Ok(Data::Posts(posts.await?))
}

pub async fn hashtag(api: &Api, tag: &str, page: &PageArgs) -> Result<Data> {
  let posts = page.collect_dated(async |r| videos::hashtag(api, tag, &r).await);
  Ok(Data::Posts(posts.await?))
}

pub async fn suggest(api: &Api, query: &str) -> Result<Data> {
  Ok(Data::Value(json!(search::suggest(api, query).await?)))
}

/// A live chat (or its replay), one page of messages at a time.
pub async fn chat(api: &Api, video: &str, page: &PageArgs) -> Result<Data> {
  let video = api.ctx.post_ref(video)?;
  let messages = page.collect(async |r| live::page(api, &video, &r).await);
  Ok(Data::Posts(messages.await?))
}
