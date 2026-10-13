//! The account's library: home and subscription feeds, subscriptions,
//! playlists (Watch later `WL`, Liked videos `LL`, own ones), history; and
//! any public playlist. Saving a video to Watch later or a playlist uses the
//! body of YouTube.js `PlaylistManager` / the web app's `playlistEditEndpoint`.

use social_core::{
  Action, Collection, Error, Page, PageReq, Post, Result, User, Value, ValueExt, json,
};

use crate::api::Api;
use crate::channels::Tab;
use crate::{account, channels, parse, refs};

pub const FEED_KINDS: &[&str] = &["home", "subscriptions"];

pub async fn feed(api: &Api, kind: Option<&str>, req: &PageReq) -> Result<Page<Post>> {
  let id = match kind.unwrap_or("home") {
    "subscriptions" => {
      api.require_login()?;
      "FEsubscriptions"
    }
    _ => "FEwhat_to_watch",
  };
  let l = parse::listing(&api.browse(id, None, req).await?);
  if l.posts.is_empty() && req.cursor.is_none() && !api.logged_in() {
    return Err(
      Error::auth("YouTube shows no home feed without a login").with_hint(api.ctx.login_hint()),
    );
  }
  Ok(Page::new(l.posts, l.next))
}

/// Refuse lists YouTube shows to their owner only.
async fn own_only(api: &Api, user: Option<&str>, what: &str) -> Result<()> {
  api.require_login()?;
  match user {
    Some(u) if !account::is_mine(api, u).await? => Err(Error::input(format!(
      "YouTube shows {what} of the logged-in account only (use `me`)"
    ))),
    _ => Ok(()),
  }
}

/// Channels the logged-in account subscribes to.
pub async fn following(api: &Api, user: &str, req: &PageReq) -> Result<Page<User>> {
  own_only(api, Some(user), "subscriptions").await?;
  let l = parse::listing(&api.browse("FEchannels", None, req).await?);
  Ok(Page::new(l.users, l.next))
}

/// Playlists of a channel, or of the logged-in account (with Watch later and Liked videos).
pub async fn collections(api: &Api, user: Option<&str>, req: &PageReq) -> Result<Page<Collection>> {
  let l = match user {
    Some(u) if !account::is_me(u) => channels::tab(api, u, Tab::Playlists, req).await?,
    _ => {
      api.require_login()?;
      parse::listing(&api.browse("FEplaylist_aggregation", None, req).await?)
    }
  };
  Ok(Page::new(l.collections, l.next))
}

/// Videos of a playlist, in playlist order.
pub async fn playlist(api: &Api, id: &str, req: &PageReq) -> Result<Page<Post>> {
  let l = parse::listing(&api.browse(&format!("VL{id}"), None, req).await?);
  Ok(Page::new(l.posts, l.next))
}

/// Watch later, or the playlist given as the folder.
pub async fn favorites(
  api: &Api,
  user: Option<&str>,
  folder: Option<&str>,
  req: &PageReq,
) -> Result<Page<Post>> {
  match folder {
    Some(f) => playlist(api, &refs::playlist(f)?, req).await,
    None => {
      own_only(api, user, "Watch later").await?;
      playlist(api, "WL", req).await
    }
  }
}

/// Liked videos (the `LL` playlist).
pub async fn likes(api: &Api, user: Option<&str>, req: &PageReq) -> Result<Page<Post>> {
  own_only(api, user, "liked videos").await?;
  playlist(api, "LL", req).await
}

pub async fn history(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let l = parse::listing(&api.browse("FEhistory", None, req).await?);
  Ok(Page::new(l.posts, l.next))
}

/// Remove a video from the watch history: its history entry carries the
/// feedback token of "Remove from watch history" (YouTube.js
/// `History.removeVideo`). Looks through the first `pages` pages.
pub async fn forget(api: &Api, arg: &str, pages: usize) -> Result<Action> {
  api.require_login()?;
  let id = refs::video(arg)?;
  let mut req = PageReq::default();
  for _ in 0..pages.max(1) {
    let l = parse::listing(&api.browse("FEhistory", None, &req).await?);
    let token = l.posts.iter().filter(|p| p.id == id).find_map(|p| {
      let raw = p.raw.as_ref()?;
      crate::parse::find(raw, "feedbackEndpoint")
        .into_iter()
        .find_map(|f| f.str("feedbackToken"))
    });
    if let Some(token) = token {
      let body = json!({
        "feedbackTokens": [token],
        "isFeedbackTokenUnencrypted": false,
        "shouldMerge": false,
      });
      let v = api.write("feedback", body).await?;
      let ok = v
        .list("feedbackResponses")
        .iter()
        .all(|r| r.bool("isProcessed") != Some(false));
      if !ok {
        return Err(Error::upstream(
          "YouTube did not remove the video from history",
        ));
      }
      return Ok(Action::done("history_remove", &id).with_url(refs::video_url(&id)));
    }
    match l.next {
      Some(next) => req.cursor = Some(next),
      None => break,
    }
  }
  Err(Error::not_found(format!(
    "{id} is not in the first {pages} pages of your watch history"
  )))
}

fn failed(v: &Value) -> bool {
  v.str("status").is_some_and(|s| s != "STATUS_SUCCEEDED")
}

/// Add to (or remove from) Watch later or the playlist named by `folder`.
pub async fn favorite(api: &Api, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  let id = refs::video(post)?;
  let list = match folder {
    Some(f) => refs::playlist(f)?,
    None => "WL".to_owned(),
  };
  let action = if undo {
    json!({ "action": "ACTION_REMOVE_VIDEO_BY_VIDEO_ID", "removedVideoId": id })
  } else {
    json!({ "action": "ACTION_ADD_VIDEO", "addedVideoId": id })
  };
  let body = json!({ "playlistId": list, "actions": [action] });
  let v = api.write("browse/edit_playlist", body).await?;
  if failed(&v) {
    return Err(Error::upstream(format!(
      "YouTube could not change playlist {list} ({})",
      v.str("status").unwrap_or_default()
    )));
  }
  let name = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(name, &id).with_url(refs::playlist_url(&list)))
}
