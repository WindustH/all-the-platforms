//! Managing the account: dislikes, comment ratings, the bell, playlists,
//! watch history, the account's channels and the one-time OAuth grant.

use atp_core::cli::confirm;
use atp_core::{Action, Data, Page, Result, json};

use crate::api::Api;
use crate::{account, channels, comments, insights, library, playlists, refs};

pub async fn oauth(api: &Api) -> Result<Data> {
  Ok(Data::Value(insights::authorize(&api.ctx).await?))
}

pub async fn accounts(api: &Api) -> Result<Data> {
  Ok(Data::Users(Page::last(account::accounts(api).await?)))
}

pub async fn dislike(api: &Api, video: &str, undo: bool) -> Result<Data> {
  let id = refs::video(&api.ctx.post_ref(video)?)?;
  let (path, name) = if undo {
    ("like/removelike", "undislike")
  } else {
    ("like/dislike", "dislike")
  };
  api
    .write(path, json!({ "target": { "videoId": id } }))
    .await?;
  Ok(Data::Action(
    Action::done(name, &id).with_url(refs::video_url(&id)),
  ))
}

pub async fn rate_comment(
  api: &Api,
  video: &str,
  comment: &str,
  dislike: bool,
  undo: bool,
) -> Result<Data> {
  let video = api.ctx.post_ref(video)?;
  let action = comments::rate(api, &video, comment, dislike, undo).await?;
  Ok(Data::Action(action))
}

pub async fn bell(api: &Api, user: &str, level: &str) -> Result<Data> {
  let user = api.ctx.user_ref(user)?;
  Ok(Data::Action(channels::bell(api, &user, level).await?))
}

pub async fn create_playlist(
  api: &Api,
  title: &str,
  videos: &[String],
  description: Option<&str>,
  privacy: &str,
) -> Result<Data> {
  let videos = video_refs(api, videos)?;
  let action = playlists::create(api, title, description, privacy, &videos).await?;
  Ok(Data::Action(action))
}

pub async fn delete_playlist(api: &Api, playlist: &str, yes: bool) -> Result<Data> {
  let playlist = api.ctx.collection_ref(playlist)?;
  confirm(yes, &format!("delete playlist {playlist}"))?;
  Ok(Data::Action(playlists::delete(api, &playlist).await?))
}

pub async fn add_to_playlist(api: &Api, playlist: &str, videos: &[String]) -> Result<Data> {
  let videos = video_refs(api, videos)?;
  let playlist = api.ctx.collection_ref(playlist)?;
  Ok(Data::Action(playlists::add(api, &playlist, &videos).await?))
}

pub async fn remove_from_playlist(api: &Api, playlist: &str, videos: &[String]) -> Result<Data> {
  let videos = video_refs(api, videos)?;
  let playlist = api.ctx.collection_ref(playlist)?;
  Ok(Data::Action(
    playlists::remove(api, &playlist, &videos).await?,
  ))
}

/// What `playlist-edit` changes; `None` keeps a field.
pub struct PlaylistChanges<'a> {
  pub title: Option<&'a str>,
  pub description: Option<&'a str>,
  pub privacy: Option<&'a str>,
}

pub async fn edit_playlist(
  api: &Api,
  playlist: &str,
  changes: &PlaylistChanges<'_>,
) -> Result<Data> {
  let playlist = api.ctx.collection_ref(playlist)?;
  let action = playlists::update(
    api,
    &playlist,
    changes.title,
    changes.description,
    changes.privacy,
  )
  .await?;
  Ok(Data::Action(action))
}

pub async fn save_playlist(api: &Api, playlist: &str, undo: bool) -> Result<Data> {
  let playlist = api.ctx.collection_ref(playlist)?;
  Ok(Data::Action(playlists::save(api, &playlist, undo).await?))
}

pub async fn forget(api: &Api, video: &str, pages: usize) -> Result<Data> {
  let video = api.ctx.post_ref(video)?;
  Ok(Data::Action(library::forget(api, &video, pages).await?))
}

/// Video arguments with `#N` resolved.
fn video_refs(api: &Api, videos: &[String]) -> Result<Vec<String>> {
  videos.iter().map(|v| api.ctx.post_ref(v)).collect()
}
