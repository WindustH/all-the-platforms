//! Managing the account: dislikes, comment ratings, the bell, the order
//! and privacy of playlists, saved playlists, watch and search history, the
//! account's channels and the one-time OAuth grant.

use atp_core::cli::confirm;
use atp_core::{Action, Data, Page, Result, json};

use crate::api::Api;
use crate::{account, channels, comments, library, oauth, playlists, refs};

pub async fn oauth(api: &Api) -> Result<Data> {
  Ok(Data::Value(oauth::authorize(&api.ctx).await?))
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

/// Move a video within one of your playlists, after another or to the top.
pub async fn move_in_playlist(
  api: &Api,
  playlist: &str,
  video: &str,
  after: Option<&str>,
) -> Result<Data> {
  let playlist = api.ctx.collection_ref(playlist)?;
  let video = api.ctx.post_ref(video)?;
  let after = after.map(|a| api.ctx.post_ref(a)).transpose()?;
  let action = playlists::reorder(api, &playlist, &video, after.as_deref()).await?;
  Ok(Data::Action(action))
}

pub async fn playlist_privacy(api: &Api, playlist: &str, level: &str) -> Result<Data> {
  let playlist = api.ctx.collection_ref(playlist)?;
  Ok(Data::Action(
    playlists::set_privacy(api, &playlist, level).await?,
  ))
}

pub async fn save_playlist(api: &Api, playlist: &str, undo: bool) -> Result<Data> {
  let playlist = api.ctx.collection_ref(playlist)?;
  Ok(Data::Action(playlists::save(api, &playlist, undo).await?))
}

/// Clear the watch history (`search`: the search history), after asking.
pub async fn clear_history(api: &Api, search: bool, yes: bool) -> Result<Data> {
  let what = if search { "search" } else { "watch" };
  confirm(yes, &format!("clear your whole {what} history"))?;
  Ok(Data::Action(library::clear(api, search).await?))
}

pub async fn forget(api: &Api, video: &str, pages: usize) -> Result<Data> {
  let video = api.ctx.post_ref(video)?;
  Ok(Data::Action(library::forget(api, &video, pages).await?))
}
