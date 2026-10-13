//! Group administration: removing, muting and naming members.

use atp_core::{Action, Error, Result, json};

use crate::api::Api;
use crate::refs::{self, Chat, num};

/// A group's number from a chat argument that must name one.
fn group(arg: &str) -> Result<String> {
  match refs::chat(arg)? {
    Chat::Group(g) => Ok(g),
    Chat::Private(_) => Err(Error::input("give a group (group:<number>)")),
  }
}

/// `(group, user)` numbers of a group and one of its members.
fn member(group_arg: &str, user_arg: &str) -> Result<(String, String)> {
  Ok((group(group_arg)?, refs::user(user_arg)?))
}

pub async fn kick(api: &Api, group: &str, user: &str, reject: bool) -> Result<Action> {
  let (g, u) = member(group, user)?;
  let params = json!({"group_id": num(&g), "user_id": num(&u), "reject_add_request": reject});
  api.call("set_group_kick", params).await?;
  Ok(Action::done("kick", format!("{u} from group:{g}")))
}

/// Mute for `minutes` (0 lifts it).
pub async fn mute(api: &Api, group: &str, user: &str, minutes: u64) -> Result<Action> {
  let (g, u) = member(group, user)?;
  let params = json!({"group_id": num(&g), "user_id": num(&u), "duration": minutes * 60});
  api.call("set_group_ban", params).await?;
  let what = if minutes == 0 { "unmute" } else { "mute" };
  Ok(Action::done(what, format!("{u} in group:{g}")))
}

/// A member's group card (nickname in the group); empty clears it.
pub async fn card(api: &Api, group: &str, user: &str, card: &str) -> Result<Action> {
  let (g, u) = member(group, user)?;
  let params = json!({"group_id": num(&g), "user_id": num(&u), "card": card});
  api.call("set_group_card", params).await?;
  Ok(Action::done("card", format!("{u} in group:{g}")))
}
