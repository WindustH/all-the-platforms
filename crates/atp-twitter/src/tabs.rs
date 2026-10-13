//! More of a profile and a tweet: the replies, media, highlights and
//! articles tabs, verified followers and followers you know, similar posts
//! and a tweet's edit history.

use atp_core::{Error, ErrorCode, Page, PageReq, Post, Result, User, json};

use crate::api::Api;
use crate::graphql::{
  EDIT_HISTORY, FOLLOWERS_YOU_KNOW, Op, SIMILAR_POSTS, USER_ARTICLES, USER_HIGHLIGHTS, USER_MEDIA,
  USER_REPLIES, VERIFIED_FOLLOWERS,
};
use crate::refs;
use crate::timeline::{self, vars, with};
use crate::users;

/// A profile tab other than the posts (`user-posts`).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Tab {
  /// Posts and replies
  Replies,
  Media,
  Highlights,
  Articles,
}

impl Tab {
  fn op(self) -> &'static Op {
    match self {
      Tab::Replies => &USER_REPLIES,
      Tab::Media => &USER_MEDIA,
      Tab::Highlights => &USER_HIGHLIGHTS,
      Tab::Articles => &USER_ARTICLES,
    }
  }
}

pub async fn tab(api: &Api, arg: &str, tab: Tab, req: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let id = users::user_id(api, arg).await?;
  let variables = with(
    vars(req),
    json!({
      "userId": id,
      "includePromotedContent": false,
      "withCommunity": true,
      "withVoice": true,
      "withClientEventToken": false,
      "withBirdwatchNotes": false,
    }),
  );
  timeline::page(
    api,
    tab.op(),
    variables,
    timeline::USER_PATHS,
    req,
    timeline::posts,
  )
  .await
}

/// `verified`: verified followers; otherwise the followers you also follow.
pub async fn followers(api: &Api, arg: &str, verified: bool, req: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let id = users::user_id(api, arg).await?;
  let op = if verified {
    &VERIFIED_FOLLOWERS
  } else {
    &FOLLOWERS_YOU_KNOW
  };
  let variables = with(
    vars(req),
    json!({ "userId": id, "includePromotedContent": false }),
  );
  timeline::page(
    api,
    op,
    variables,
    timeline::USER_PATHS,
    req,
    timeline::users,
  )
  .await
}

/// Posts X considers similar to a tweet (Premium accounts see them).
pub async fn similar(api: &Api, arg: &str) -> Result<Page<Post>> {
  api.require_login()?;
  let id = refs::tweet_id(arg)?;
  let data = api
    .graphql(&SIMILAR_POSTS, json!({ "tweet_id": id }))
    .await?;
  let tl = timeline::Timeline::at(
    &data,
    &["data.tweet_result_by_rest_id.result.similar_posts_timeline.timeline.instructions"],
  );
  let paywall = tl.entries.iter().any(|e| e.id.contains("paywall"));
  let posts = timeline::posts(&tl);
  if posts.is_empty() && paywall {
    return Err(
      Error::new(
        ErrorCode::PermissionDenied,
        "similar posts are an X Premium feature",
      )
      .with_hint("X shows them to Premium subscribers only"),
    );
  }
  Ok(Page::last(posts))
}

/// Every version of an edited tweet, newest first.
pub async fn edits(api: &Api, arg: &str) -> Result<Page<Post>> {
  api.require_login()?;
  let id = refs::tweet_id(arg)?;
  let variables = json!({ "tweetId": id, "withQuickPromoteEligibilityTweetFields": true });
  let data = api.graphql(&EDIT_HISTORY, variables).await?;
  let tl = timeline::Timeline::at(
    &data,
    &[
      "data.tweet_result_by_rest_id.result.edit_history_timeline.timeline.instructions",
      "data.tweetResult.result.edit_history_timeline.timeline.instructions",
    ],
  );
  Ok(Page::last(timeline::posts(&tl)))
}
