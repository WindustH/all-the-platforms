//! The logged-in account's own things: blocks and mutes, the pinned tweet,
//! scheduled posts, bookmark folders, unread counters, and trends by place.

use std::collections::BTreeMap;

use social_core::text::from_secs;
use social_core::{
  Action, Collection, Error, Page, PageReq, Post, Result, User, Value, ValueExt, json,
};

use crate::api::{Api, REST};
use crate::graphql::{
  BLOCKED, CREATE_SCHEDULED, DELETE_SCHEDULED, FOLDER_ADD, FOLDER_CREATE, FOLDER_DELETE,
  FOLDER_REMOVE, FOLDER_RENAME, MUTED, Op, PIN_TWEET, SCHEDULED, UNPIN_TWEET,
};
use crate::refs::{self, user_url};
use crate::timeline::{self, vars, with};
use crate::users;

async fn mutate(api: &Api, op: &Op, variables: Value) -> Result<Value> {
  api.require_login()?;
  api.write_pause().await;
  api.graphql(op, variables).await
}

/// Block (or unblock) / mute (or unmute) a user.
pub async fn block(api: &Api, user: &str, mute: bool, undo: bool) -> Result<Action> {
  api.require_login()?;
  let id = users::user_id(api, user).await?;
  let (path, name) = match (mute, undo) {
    (false, false) => ("blocks/create", "block"),
    (false, true) => ("blocks/destroy", "unblock"),
    (true, false) => ("mutes/users/create", "mute"),
    (true, true) => ("mutes/users/destroy", "unmute"),
  };
  api.write_pause().await;
  let v = api
    .post_form(&format!("{REST}/{path}.json"), &[("user_id", id.as_str())])
    .await?;
  let mut action = Action::done(name, &id);
  if let Some(handle) = v.str("screen_name") {
    action = action.with_url(user_url(&handle));
  }
  Ok(action)
}

/// Accounts you block, or with `muted` the ones you mute.
pub async fn blocked(api: &Api, muted: bool, req: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let (op, paths): (&Op, &[&str]) = if muted {
    (
      &MUTED,
      &["data.viewer.muting_timeline.timeline.instructions"],
    )
  } else {
    (
      &BLOCKED,
      &[
        "data.viewer.timeline.timeline.instructions",
        "data.viewer.blocking_timeline.timeline.instructions",
      ],
    )
  };
  let variables = with(vars(req), json!({ "includePromotedContent": false }));
  timeline::page(api, op, variables, paths, req, timeline::users).await
}

/// Pin a tweet to your profile, or unpin it.
pub async fn pin(api: &Api, post: &str, undo: bool) -> Result<Action> {
  let id = refs::tweet_id(post)?;
  let (op, name) = if undo {
    (&UNPIN_TWEET, "unpin")
  } else {
    (&PIN_TWEET, "pin")
  };
  mutate(api, op, json!({ "tweet_id": id })).await?;
  Ok(Action::done(name, &id).with_url(format!("https://x.com/i/status/{id}")))
}

/// Your scheduled posts, soonest first.
pub async fn scheduled(api: &Api) -> Result<Page<Post>> {
  api.require_login()?;
  let data = api
    .graphql(&SCHEDULED, json!({ "ascending": true }))
    .await?;
  let items = data
    .list("data.viewer.scheduled_tweet_list")
    .iter()
    .filter_map(|s| {
      let id = s.str("rest_id")?;
      let mut post = Post {
        kind: "scheduled".into(),
        text: s.str("tweet_create_request.status"),
        created_at: s.i64("scheduling_info.execute_at").and_then(from_secs),
        raw: Some(s.clone()),
        id,
        ..Post::default()
      };
      if let Some(state) = s.str("scheduling_info.state") {
        post.extra.insert("state".into(), state.into());
      }
      Some(post)
    })
    .collect();
  Ok(Page::last(items))
}

/// Schedule a text post for `at` (unix seconds).
pub async fn schedule(api: &Api, text: &str, at: i64, media: &[String]) -> Result<Action> {
  let variables = json!({
    "post_tweet_request": {
      "auto_populate_reply_metadata": false,
      "status": text,
      "exclude_reply_user_ids": [],
      "media_ids": media,
    },
    "execute_at": at,
  });
  let data = mutate(api, &CREATE_SCHEDULED, variables).await?;
  let mut action = Action::done("schedule", text);
  if let Some(id) = data.str("data.tweet.rest_id") {
    action = action.with_id(id);
  }
  Ok(action)
}

pub async fn unschedule(api: &Api, id: &str) -> Result<Action> {
  mutate(api, &DELETE_SCHEDULED, json!({ "scheduled_tweet_id": id })).await?;
  Ok(Action::done("unschedule", id))
}

/// Put a bookmarked tweet into a folder (or take it out with `undo`).
pub async fn file(api: &Api, post: &str, folder: &str, undo: bool) -> Result<Action> {
  let id = refs::tweet_id(post)?;
  let (op, name) = if undo {
    (&FOLDER_REMOVE, "unfile")
  } else {
    (&FOLDER_ADD, "file")
  };
  mutate(
    api,
    op,
    json!({ "tweet_id": id, "bookmark_collection_id": folder }),
  )
  .await?;
  Ok(Action::done(name, &id).with_message(format!("folder {folder}")))
}

pub async fn create_folder(api: &Api, name: &str) -> Result<Collection> {
  let data = mutate(api, &FOLDER_CREATE, json!({ "name": name })).await?;
  let f = data.at("data.bookmark_collection_create");
  Ok(Collection {
    id: f
      .str("id")
      .ok_or_else(|| Error::upstream("X did not create the folder"))?,
    kind: "folder".into(),
    name: f.str("name").unwrap_or_else(|| name.to_owned()),
    ..Collection::default()
  })
}

pub async fn rename_folder(api: &Api, folder: &str, name: &str) -> Result<Action> {
  mutate(
    api,
    &FOLDER_RENAME,
    json!({ "bookmark_collection_id": folder, "name": name }),
  )
  .await?;
  Ok(Action::done("rename-folder", folder).with_message(name.to_owned()))
}

pub async fn delete_folder(api: &Api, folder: &str) -> Result<Action> {
  mutate(
    api,
    &FOLDER_DELETE,
    json!({ "bookmark_collection_id": folder }),
  )
  .await?;
  Ok(Action::done("delete-folder", folder))
}

/// Unread notifications and direct messages.
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  api.require_login()?;
  let v = api
    .get(
      "https://x.com/i/api/2/badge_count/badge_count.json",
      &[("supports_ntab_urt", "1")],
    )
    .await?;
  let mut counts = BTreeMap::new();
  for (key, path) in [
    ("notifications", "ntab_unread_count"),
    ("messages", "dm_unread_count"),
    ("total", "total_unread_count"),
  ] {
    if let Some(n) = v.u64(path) {
      counts.insert(key.to_owned(), n);
    }
  }
  Ok(counts)
}

/// Places with trends (`woeid` for `trends --place`).
pub async fn places(api: &Api) -> Result<Page<Collection>> {
  api.require_login()?;
  let v = api
    .get(&format!("{REST}/trends/available.json"), &[])
    .await?;
  let items = v
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|p| {
      let mut c = Collection {
        id: p.str("woeid")?,
        kind: p
          .str("placeType.name")
          .unwrap_or_else(|| "place".into())
          .to_lowercase(),
        name: p.str("name").unwrap_or_default(),
        ..Collection::default()
      };
      if let Some(country) = p.str("country").filter(|c| !c.is_empty()) {
        c.extra.insert("country".into(), country.into());
      }
      Some(c)
    })
    .collect();
  Ok(Page::last(items))
}

/// The top trends of one place (`1` is worldwide), with tweet volumes.
pub async fn place_trends(api: &Api, woeid: &str) -> Result<Page<Post>> {
  api.require_login()?;
  let v = api
    .get(&format!("{REST}/trends/place.json"), &[("id", woeid)])
    .await?;
  let block = v.at("0");
  let items = block
    .list("trends")
    .iter()
    .enumerate()
    .filter_map(|(i, t)| {
      let name = t.str("name")?;
      let mut post = Post {
        kind: "trend".into(),
        title: Some(name.clone()),
        url: t.str("url"),
        raw: Some(t.clone()),
        id: name,
        ..Post::default()
      };
      if let Some(volume) = t.u64("tweet_volume") {
        post.metrics.other.insert("tweets".into(), volume);
      }
      post.extra.insert("rank".into(), (i as u64 + 1).into());
      Some(post)
    })
    .collect();
  Ok(Page::last(items))
}
