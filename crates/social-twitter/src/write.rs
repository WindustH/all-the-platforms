//! Writing: likes, bookmarks, retweets, posting / replying / quoting,
//! deleting and following. Every write pauses briefly first.

use social_core::{Action, Draft, Error, Reply, Result, Value, ValueExt, json};

use crate::api::{Api, REST};
use crate::graphql::{
  BOOKMARK, CREATE_TWEET, DELETE_TWEET, FAVORITE, RETWEET, UNBOOKMARK, UNFAVORITE, UNRETWEET,
};
use crate::refs::{self, tweet_url};
use crate::uploads;
use crate::users;

/// Most images one tweet can carry.
pub async fn like(api: &Api, post: &str, undo: bool) -> Result<Action> {
  let id = refs::tweet_id(post)?;
  let (op, variables, name) = if undo {
    (
      &UNFAVORITE,
      json!({ "tweet_id": id, "dark_request": false }),
      "unlike",
    )
  } else {
    (&FAVORITE, json!({ "tweet_id": id }), "like")
  };
  api.mutate(op, variables).await?;
  Ok(Action::done(name, &id).with_url(tweet_url("i", &id)))
}

pub async fn bookmark(api: &Api, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  if folder.is_some() {
    return Err(Error::input(
      "choosing a bookmark folder is not supported; bookmark first, then move it in the app",
    ));
  }
  let id = refs::tweet_id(post)?;
  let (op, name) = if undo {
    (&UNBOOKMARK, "unbookmark")
  } else {
    (&BOOKMARK, "bookmark")
  };
  api.mutate(op, json!({ "tweet_id": id })).await?;
  Ok(Action::done(name, &id).with_url(tweet_url("i", &id)))
}

pub async fn retweet(api: &Api, post: &str, undo: bool) -> Result<Action> {
  let id = refs::tweet_id(post)?;
  if undo {
    let variables = json!({ "source_tweet_id": id, "dark_request": false });
    api.mutate(&UNRETWEET, variables).await?;
    return Ok(Action::done("unretweet", &id).with_url(tweet_url("i", &id)));
  }
  let data = api
    .mutate(&RETWEET, json!({ "tweet_id": id, "dark_request": false }))
    .await?;
  let mut action = Action::done("retweet", &id).with_url(tweet_url("i", &id));
  if let Some(new) = data.str("data.create_retweet.retweet_results.result.rest_id") {
    action = action.with_id(new);
  }
  Ok(action)
}

/// What a new tweet is attached to.
#[derive(Default)]
pub struct Links {
  pub reply_to: Option<String>,
  pub quote: Option<String>,
  /// The tweet this one replaces (an edit).
  pub edit: Option<String>,
}

/// `CreateTweet`; returns the new tweet id.
pub async fn create(api: &Api, text: &str, media_ids: &[String], links: &Links) -> Result<String> {
  let entities: Vec<Value> = media_ids
    .iter()
    .map(|id| json!({ "media_id": id, "tagged_users": [] }))
    .collect();
  let mut variables = json!({
    "tweet_text": text,
    "media": { "media_entities": entities, "possibly_sensitive": false },
    "semantic_annotation_ids": [],
    "dark_request": false,
  });
  if let Some(id) = &links.reply_to {
    variables["reply"] = json!({ "in_reply_to_tweet_id": id, "exclude_reply_user_ids": [] });
  }
  if let Some(id) = &links.quote {
    variables["attachment_url"] = tweet_url("i", id).into();
  }
  if let Some(id) = &links.edit {
    variables["edit_options"] = json!({ "previous_tweet_id": id });
  }
  let data = api.mutate(&CREATE_TWEET, variables).await?;
  data
    .str("data.create_tweet.tweet_results.result.rest_id")
    .ok_or_else(|| Error::upstream("X did not create the tweet"))
}

pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  let mut text = match &draft.title {
    Some(title) if !draft.text.is_empty() => format!("{title}\n\n{}", draft.text),
    Some(title) => title.clone(),
    None => draft.text.clone(),
  };
  for topic in &draft.topics {
    text.push_str(&format!(" #{}", topic.trim_start_matches('#')));
  }
  let links = Links {
    reply_to: draft.reply_to.as_deref().map(refs::tweet_id).transpose()?,
    quote: draft.quote.as_deref().map(refs::tweet_id).transpose()?,
    edit: None,
  };
  let media = uploads::images(api, &draft.images).await?;
  let id = create(api, text.trim(), &media, &links).await?;
  let name = if links.reply_to.is_some() {
    "reply"
  } else if links.quote.is_some() {
    "quote"
  } else {
    "publish"
  };
  let target = links.reply_to.or(links.quote).unwrap_or_else(|| id.clone());
  Ok(
    Action::done(name, target)
      .with_id(&id)
      .with_url(tweet_url("i", &id)),
  )
}

/// Reply to `post`, or to one of its replies (`reply_to`).
/// A reply tweet under the post, or under one of its replies.
pub async fn reply(api: &Api, post: &str, reply: &Reply) -> Result<Action> {
  let target = refs::tweet_id(reply.reply_to.as_deref().unwrap_or(post))?;
  let links = Links {
    reply_to: Some(target.clone()),
    ..Links::default()
  };
  let media = uploads::images(api, &reply.images).await?;
  let id = create(api, &reply.text, &media, &links).await?;
  Ok(
    Action::done("comment", target)
      .with_id(&id)
      .with_url(tweet_url("i", &id)),
  )
}

/// Edit one of your tweets (X Premium, within an hour of posting, five
/// times at most): a new version replaces it.
pub async fn edit(api: &Api, post: &str, text: &str) -> Result<Action> {
  let id = refs::tweet_id(post)?;
  let links = Links {
    edit: Some(id.clone()),
    ..Links::default()
  };
  let new = create(api, text, &[], &links).await?;
  Ok(
    Action::done("edit", &id)
      .with_id(&new)
      .with_url(tweet_url("i", &new)),
  )
}

/// Delete one of your tweets (a post or a reply).
pub async fn delete(api: &Api, post: &str, name: &str) -> Result<Action> {
  let id = refs::tweet_id(post)?;
  api
    .mutate(
      &DELETE_TWEET,
      json!({ "tweet_id": id, "dark_request": false }),
    )
    .await?;
  Ok(Action::done(name, id))
}

pub async fn follow(api: &Api, user: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let id = users::user_id(api, user).await?;
  let (endpoint, name) = if undo {
    ("destroy", "unfollow")
  } else {
    ("create", "follow")
  };
  api.write_pause().await;
  let form = [
    ("user_id", id.as_str()),
    ("include_profile_interstitial_type", "1"),
  ];
  let v = api
    .post_form(&format!("{REST}/friendships/{endpoint}.json"), &form)
    .await?;
  let mut action = Action::done(name, &id);
  if let Some(handle) = v.str("screen_name") {
    action = action.with_url(refs::user_url(&handle));
  }
  Ok(action)
}
