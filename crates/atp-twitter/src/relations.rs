//! Who follows you: removing followers (`RemoveFollower`) and the follow
//! requests of a protected account (`friendships/incoming`, `accept`,
//! `deny`), as the web app's follower-requests page sends them.

use atp_core::bulk;
use atp_core::{Action, Page, PageReq, Result, User, Value, ValueExt, json};

use crate::api::{Api, REST};
use crate::graphql::REMOVE_FOLLOWER;
use crate::parse;
use crate::refs::user_url;
use crate::users;

/// The user fields the web app asks for along with friendship calls.
const INCLUDE: &[(&str, &str)] = &[
  ("include_profile_interstitial_type", "1"),
  ("include_blocking", "1"),
  ("include_blocked_by", "1"),
  ("include_followed_by", "1"),
  ("include_want_retweets", "1"),
  ("include_mute_edge", "1"),
  ("include_can_dm", "1"),
  ("include_can_media_tag", "1"),
  ("include_ext_is_blue_verified", "1"),
  ("include_ext_verified_type", "1"),
  ("skip_status", "1"),
];

/// Requesters looked up at once (`users/lookup`), as the web app does.
const LOOKUP: usize = 20;

/// Make users stop following you, one call each.
pub async fn remove_followers(api: &Api, inputs: &[String]) -> Result<Vec<Action>> {
  api.require_login()?;
  bulk::each("remove-follower", inputs, async |input| {
    let id = users::user_id(api, input).await?;
    api
      .mutate(&REMOVE_FOLLOWER, json!({ "target_user_id": id }))
      .await?;
    Ok(Action::done("remove-follower", &id))
  })
  .await
}

/// Accounts asking to follow your protected account: a page of ids
/// (`friendships/incoming`, 100 at a time), filled in 20 at a time.
pub async fn requests(api: &Api, req: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let cursor = req.cursor.as_deref().unwrap_or("-1");
  let mut query = INCLUDE.to_vec();
  query.extend([
    ("cursor", cursor),
    ("stringify_ids", "true"),
    ("count", "100"),
  ]);
  let v = api
    .get(&format!("{REST}/friendships/incoming.json"), &query)
    .await?;
  let ids: Vec<String> = v
    .list("ids")
    .iter()
    .filter_map(|id| match id {
      Value::String(s) => Some(s.clone()),
      Value::Number(n) => Some(n.to_string()),
      _ => None,
    })
    .collect();
  let mut found = Vec::with_capacity(ids.len());
  for chunk in ids.chunks(LOOKUP) {
    found.extend(lookup(api, chunk).await?);
  }
  let next = v
    .str("next_cursor_str")
    .filter(|c| c != "0" && !ids.is_empty());
  Ok(Page::new(found, next))
}

/// Users by id (`users/lookup`), in the order asked.
async fn lookup(api: &Api, ids: &[String]) -> Result<Vec<User>> {
  let joined = ids.join(",");
  let mut query = INCLUDE.to_vec();
  query.push(("user_id", &joined));
  let v = api
    .get(&format!("{REST}/users/lookup.json"), &query)
    .await?;
  let mut users: Vec<User> = v
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(parse::rest_user)
    .collect();
  users.sort_by_key(|u| ids.iter().position(|id| *id == u.id));
  Ok(users)
}

/// Accept follow requests (`accept`), or decline them, one call each.
pub async fn answer(api: &Api, inputs: &[String], accept: bool) -> Result<Vec<Action>> {
  api.require_login()?;
  let (path, name) = if accept {
    ("friendships/accept", "accept-follower")
  } else {
    ("friendships/deny", "deny-follower")
  };
  bulk::each(name, inputs, async |input| {
    let id = users::user_id(api, input).await?;
    api.write_pause().await;
    let mut form = INCLUDE.to_vec();
    form.extend([("cursor", "-1"), ("user_id", id.as_str())]);
    let v = api.post_form(&format!("{REST}/{path}.json"), &form).await?;
    let mut action = Action::done(name, &id);
    if let Some(handle) = v.str("screen_name") {
      action = action.with_url(user_url(&handle));
    }
    Ok(action)
  })
  .await
}
