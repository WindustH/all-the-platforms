//! Changing relations as weibo.com's relation pages do: unfollowing
//! several people at once, removing followers, blocks (黑名单), special
//! follows (特别关注) and remarks (备注).

use atp_core::bulk;
use atp_core::{Action, Error, Page, PageReq, Result, User, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, UserRef, user_url};
use crate::{interactions, users};

/// People the relation page lets you select for one batch unfollow.
const BATCH: usize = 20;

/// Every user argument as a uid, all of them before anything is sent.
async fn uids(api: &Api, inputs: &[String]) -> Result<Vec<String>> {
  let mut out = Vec::with_capacity(inputs.len());
  for input in inputs {
    out.push(users::uid(api, input).await?);
  }
  Ok(out)
}

/// Unfollow several users 20 at a time through the relation page's batch
/// manager (`destroyBatch`); following goes one by one.
pub async fn follow_users(api: &Api, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  if !undo {
    return bulk::each("follow", inputs, async |u| {
      interactions::follow(api, u, false).await
    })
    .await;
  }
  api.require_login()?;
  let ids = uids(api, inputs).await?;
  bulk::batches("unfollow", &ids, BATCH, String::clone, async |chunk| {
    api.write_pause().await;
    api
      .post_json(
        "/ajax/profile/destroyBatch",
        &json!({"uids": chunk.join(",")}),
      )
      .await?;
    Ok(chunk.iter().map(|uid| done("unfollow", uid)).collect())
  })
  .await
}

fn done(action: &str, uid: &str) -> Action {
  Action::done(action, uid).with_url(user_url(uid))
}

/// Make users stop following you (移除粉丝), one call each.
pub async fn remove_followers(api: &Api, inputs: &[String]) -> Result<Vec<Action>> {
  api.require_login()?;
  bulk::each("remove-follower", inputs, async |input| {
    let uid = users::uid(api, input).await?;
    api.write_pause().await;
    api
      .post_json("/ajax/profile/destroyFollowers", &json!({"uid": uid}))
      .await?;
    Ok(done("remove-follower", &uid))
  })
  .await
}

/// Block users (拉黑: their posts hidden, no interaction, no following
/// you), or with `undo` take them off the blacklist. Screen names go as
/// they are, as the settings page sends them.
pub async fn block(api: &Api, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  api.require_login()?;
  let action = if undo { "unblock" } else { "block" };
  bulk::each(action, inputs, async |input| {
    let (key, who) = match refs::user(input)? {
      UserRef::Uid(uid) => ("uid", uid),
      UserRef::Name(name) => ("screen_name", name),
      UserRef::Domain(_) => ("uid", users::uid(api, input).await?),
    };
    api.write_pause().await;
    let path = match (undo, key) {
      (false, _) => "/ajax/statuses/filterUser",
      (true, "uid") => "/ajax/statuses/deleteFilters",
      (true, _) => "/ajax/setting/deleteFilteredUser",
    };
    let body = if undo {
      json!({ key: who })
    } else {
      json!({ key: who, "status": 1, "interact": 1, "follow": 1 })
    };
    api.post_json(path, &body).await?;
    let url = match key {
      "uid" => user_url(&who),
      _ => format!("https://weibo.com/n/{who}"),
    };
    Ok(Action::done(action, &who).with_url(url))
  })
  .await
}

/// The users you block, as the settings page lists them.
pub async fn blocked(api: &Api, page: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let no = page.number_or(1);
  let no_text = no.to_string();
  let v = api
    .get("/ajax/setting/getFilteredUsers", &[("page", &no_text)])
    .await?;
  let users: Vec<User> = v
    .list("card_group")
    .iter()
    .filter_map(parse::blocked)
    .collect();
  let more = !users.is_empty() && v.i64("next_cursor").is_some_and(|n| n != 0);
  Ok(Page::new(users, more.then(|| (no + 1).to_string())))
}

/// Mark users as special follows (特别关注), or with `undo` unmark them.
pub async fn special(api: &Api, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  api.require_login()?;
  let (action, path) = if undo {
    ("unspecial", "/ajax/friendships/specialDestory")
  } else {
    ("special", "/ajax/friendships/specialAdd")
  };
  bulk::each(action, inputs, async |input| {
    let uid = users::uid(api, input).await?;
    api.write_pause().await;
    let v = api.post_json(path, &json!({"touid": uid})).await?;
    if v.bool("result") == Some(false) {
      return Err(Error::upstream(format!(
        "Weibo did not change the special follow of {uid}"
      )));
    }
    Ok(done(action, &uid))
  })
  .await
}

/// Set the remark (备注) you see instead of a user's name; empty clears it.
pub async fn remark(api: &Api, input: &str, text: &str) -> Result<Action> {
  api.require_login()?;
  let uid = users::uid(api, input).await?;
  api.write_pause().await;
  api
    .post_json(
      "/ajax/friendships/remarkUpdate",
      &json!({"uid": uid, "remark": text}),
    )
    .await?;
  let message = if text.is_empty() {
    "remark cleared".to_owned()
  } else {
    format!("remark {text}")
  };
  Ok(done("remark", &uid).with_message(message))
}
