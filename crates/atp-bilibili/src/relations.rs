//! Changing relations: follow / unfollow (several users in one batch
//! call), blocks and the blacklist, removing followers, special follows,
//! and the followings shared with another user.

use atp_core::bulk;
use atp_core::{Action, Error, Page, PageReq, Result, User, ValueExt, json};

use crate::api::Api;
use crate::paging::Pn;
use crate::parse::{self, Rows};
use crate::refs::space_url;
use crate::users;

const MODIFY: &str = "https://api.bilibili.com/x/relation/modify";
const BATCH_MODIFY: &str = "https://api.bilibili.com/x/relation/batch/modify";
const SPECIAL_ADD: &str = "https://api.bilibili.com/x/relation/tag/special/add";
const SPECIAL_DEL: &str = "https://api.bilibili.com/x/relation/tag/special/del";
const BLACKS: &str = "https://api.bilibili.com/x/relation/blacks";
const SAME: &str = "https://api.bilibili.com/x/relation/same/followings";

/// Users one `batch/modify` call takes.
const BATCH: usize = 50;

/// What the space page reports with every relation change.
const STATISTICS: &str = r#"{"appId":100,"platform":5}"#;

/// The `act` of `/x/relation/modify`.
#[derive(Debug, Clone, Copy)]
enum Act {
  Follow = 1,
  Unfollow = 2,
  Block = 5,
  Unblock = 6,
  RemoveFollower = 7,
}

impl Act {
  fn name(self) -> &'static str {
    match self {
      Act::Follow => "follow",
      Act::Unfollow => "unfollow",
      Act::Block => "block",
      Act::Unblock => "unblock",
      Act::RemoveFollower => "remove-follower",
    }
  }
}

/// One relation change, sent with the fields the space page sends.
async fn modify(api: &Api, input: &str, act: Act) -> Result<Action> {
  let mid = users::mid(api, input).await?;
  let entity = json!({ "entity": "user", "entity_id": mid.parse::<u64>().unwrap_or_default() });
  api
    .post(MODIFY)
    .query("statistics", STATISTICS)
    .arg("fid", &mid)
    .arg("act", act as u8)
    .arg("re_src", 11)
    .arg("gaia_source", "web_main")
    .arg("spmid", "333.1387.0.0")
    .arg("extend_content", entity.to_string())
    .send()
    .await?;
  Ok(Action::done(act.name(), &mid).with_url(space_url(&mid)))
}

pub async fn follow(api: &Api, input: &str, undo: bool) -> Result<Action> {
  modify(api, input, if undo { Act::Unfollow } else { Act::Follow }).await
}

/// Follow several users, 50 per call; unfollowing goes one by one.
pub async fn follow_users(api: &Api, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  if undo {
    return bulk::each("unfollow", inputs, async |u| follow(api, u, true).await).await;
  }
  in_batches(api, inputs, Act::Follow).await
}

/// Block (or with `undo` unblock) users: several blocks go 50 per call.
pub async fn block(api: &Api, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  let act = if undo { Act::Unblock } else { Act::Block };
  if undo || inputs.len() == 1 {
    return bulk::each(act.name(), inputs, async |u| modify(api, u, act).await).await;
  }
  in_batches(api, inputs, act).await
}

/// `batch/modify` (follow or block only): every user resolved first, then
/// 50 at a time; the users it names in `failed_fids` failed.
async fn in_batches(api: &Api, inputs: &[String], act: Act) -> Result<Vec<Action>> {
  let mids = users::mids(api, inputs).await?;
  let name = act.name();
  bulk::batches(name, &mids, BATCH, String::clone, async |chunk| {
    let data = api
      .post(BATCH_MODIFY)
      .arg("fids", chunk.join(","))
      .arg("act", act as u8)
      .arg("re_src", 11)
      .send()
      .await?;
    let failed: Vec<String> = data
      .list("failed_fids")
      .iter()
      .filter_map(|f| f.as_u64().map(|f| f.to_string()))
      .collect();
    let refused = Error::upstream(format!("Bilibili did not {name} this user"));
    let one = |mid: &String| {
      if failed.contains(mid) {
        Action::failed(name, mid, &refused)
      } else {
        Action::done(name, mid).with_url(space_url(mid))
      }
    };
    Ok(chunk.iter().map(one).collect())
  })
  .await
}

/// Make users stop following you, one by one.
pub async fn remove_followers(api: &Api, inputs: &[String]) -> Result<Vec<Action>> {
  let act = Act::RemoveFollower;
  bulk::each(act.name(), inputs, async |u| modify(api, u, act).await).await
}

/// The users you blocked, with `extra.blocked_at`.
pub async fn blocked(api: &Api, page: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let pn = Pn::of(page, 50);
  let data = api
    .get(BLACKS)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .send()
    .await?;
  let users = data
    .list("list")
    .iter()
    .map(|v| {
      let mut u = parse::relation(v, Rows::Others);
      if let Some(at) = u.extra.remove("followed_at") {
        u.extra.insert("blocked_at".into(), at);
      }
      u
    })
    .collect();
  Ok(pn.page(users, pn.before(data.u64("total"))))
}

/// Mark (or with `undo` unmark) a user as a special follow (特别关注).
pub async fn special(api: &Api, input: &str, undo: bool) -> Result<Action> {
  let mid = users::mid(api, input).await?;
  api
    .post(if undo { SPECIAL_DEL } else { SPECIAL_ADD })
    .arg("fid", &mid)
    .send()
    .await?;
  let action = if undo { "unspecial" } else { "special" };
  Ok(Action::done(action, &mid).with_url(space_url(&mid)))
}

/// People both you and `input` follow.
pub async fn shared(api: &Api, input: &str, page: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let mid = users::mid(api, input).await?;
  let pn = Pn::of(page, 50);
  let data = api
    .get(SAME)
    .arg("vmid", &mid)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .send()
    .await?;
  let users = data
    .list("list")
    .iter()
    .map(|v| parse::relation(v, Rows::Others))
    .collect();
  Ok(pn.page(users, pn.before(data.u64("total"))))
}
