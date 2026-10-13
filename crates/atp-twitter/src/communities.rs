//! Communities: one community, its posts (relevance, recency or media), its
//! members and moderators, the communities you belong to, joining and leaving.

use atp_core::{Action, Collection, Error, Page, PageReq, Post, Result, User, ValueExt, json};

use crate::api::Api;
use crate::graphql::{
  COMMUNITY, COMMUNITY_MEDIA, COMMUNITY_MEMBERS, COMMUNITY_MEMBERSHIPS, COMMUNITY_MODERATORS,
  COMMUNITY_TWEETS, JOIN_COMMUNITY, LEAVE_COMMUNITY,
};
use crate::parse;
use crate::refs::{self, community_url};
use crate::timeline::{self, vars, with};

/// Order of a community's posts.
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum Sort {
  /// What X ranks first
  #[default]
  Top,
  Latest,
  /// Posts with photos or videos
  Media,
}

pub async fn info(api: &Api, arg: &str) -> Result<Collection> {
  api.require_login()?;
  let id = refs::community_id(arg)?;
  let variables = json!({
    "communityId": id,
    "withDmMuting": false,
    "withSafetyModeUserFields": false,
  });
  let data = api.graphql(&COMMUNITY, variables).await?;
  parse::community(data.at("data.communityResults.result"))
    .ok_or_else(|| Error::not_found(format!("no community {id}")))
}

pub async fn posts(api: &Api, arg: &str, sort: Sort, req: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let id = refs::community_id(arg)?;
  let (op, extra, path) = match sort {
    Sort::Media => (
      &COMMUNITY_MEDIA,
      json!({}),
      "data.communityResults.result.community_media_timeline.timeline.instructions",
    ),
    Sort::Top | Sort::Latest => (
      &COMMUNITY_TWEETS,
      json!({
        "displayLocation": "Community",
        "rankingMode": if matches!(sort, Sort::Latest) { "Recency" } else { "Relevance" },
      }),
      "data.communityResults.result.ranked_community_timeline.timeline.instructions",
    ),
  };
  let variables = with(
    with(
      vars(req),
      json!({ "communityId": id, "withCommunity": true }),
    ),
    extra,
  );
  timeline::page(api, op, variables, &[path], req, timeline::posts).await
}

/// Members, or with `moderators` the moderators, of a community.
pub async fn members(api: &Api, arg: &str, moderators: bool, req: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let id = refs::community_id(arg)?;
  let (op, slice) = if moderators {
    (&COMMUNITY_MODERATORS, "moderators_slice")
  } else {
    (&COMMUNITY_MEMBERS, "members_slice")
  };
  let mut variables = json!({ "communityId": id, "count": req.size_within(timeline::PAGE_MAX) });
  if let Some(c) = &req.cursor {
    variables["cursor"] = c.as_str().into();
  }
  let data = api.graphql(op, variables).await?;
  let s = data.at(&format!("data.communityResults.result.{slice}"));
  let items: Vec<User> = s
    .list("items_results")
    .iter()
    .filter_map(|u| parse::user(u.at("result")))
    .collect();
  let next = s
    .str("slice_info.next_cursor")
    .filter(|c| Some(c) != req.cursor.as_ref() && !items.is_empty());
  Ok(Page::new(items, next))
}

/// Communities the logged-in account belongs to.
pub async fn mine(api: &Api) -> Result<Page<Collection>> {
  api.require_login()?;
  let data = api.graphql(&COMMUNITY_MEMBERSHIPS, json!({})).await?;
  let mut found = Vec::new();
  parse::communities(&data, &mut found);
  Ok(Page::last(found))
}

/// Join a community (or leave it with `undo`). Communities that review
/// members answer with a request instead.
pub async fn join(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let id = refs::community_id(arg)?;
  let (op, name) = if undo {
    (&LEAVE_COMMUNITY, "leave")
  } else {
    (&JOIN_COMMUNITY, "join")
  };
  api.write_pause().await;
  api.graphql(op, json!({ "communityId": id })).await?;
  Ok(Action::done(name, &id).with_url(community_url(&id)))
}
