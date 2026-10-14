//! OneBot-only commands: group administration and any other action.

use atp_core::{Data, Error, Result, Value};

use crate::api::Api;
use crate::groups;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Remove a member from a group
  Kick {
    /// Group: `group:<number>` or #N
    group: String,
    /// Member: QQ number or #N
    user: String,
    /// Also refuse their future join requests
    #[arg(long)]
    reject: bool,
  },
  /// Mute a group member for some minutes (0 lifts it)
  #[command(after_long_help = EX_MUTE)]
  Mute {
    /// Group: `group:<number>` or #N
    group: String,
    /// Member: QQ number or #N
    user: String,
    /// Minutes to mute; 0 lifts the mute
    #[arg(long, default_value_t = 10)]
    minutes: u64,
  },
  /// Set a member's group card (nickname in the group); empty clears it
  Card {
    /// Group: `group:<number>` or #N
    group: String,
    /// Member: QQ number or #N
    user: String,
    /// The new card; '' clears it
    card: String,
  },
  /// Call any OneBot action with JSON parameters
  #[command(after_long_help = EX_CALL)]
  Call {
    /// Action name, e.g. `get_group_info`
    action: String,
    /// Parameters as a JSON object
    #[arg(default_value = "{}")]
    params: String,
  },
}

const EX_MUTE: &str = "\
Examples:
  atp onebot mute group:123456 10001 --minutes 30
  atp onebot mute group:123456 10001 --minutes 0   # lift the mute

More: atp guide onebot mute";

const EX_CALL: &str = "\
Examples:
  atp onebot call get_status
  atp onebot call get_group_info '{\"group_id\": 123456}'
  atp onebot call send_like '{\"user_id\": 10001, \"times\": 1}'

More: atp guide onebot call";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Kick {
      group,
      user,
      reject,
    } => {
      let (group, user) = (ctx.collection_ref(&group)?, ctx.user_ref(&user)?);
      Data::Action(groups::kick(api, &group, &user, reject).await?)
    }
    Command::Mute {
      group,
      user,
      minutes,
    } => {
      let (group, user) = (ctx.collection_ref(&group)?, ctx.user_ref(&user)?);
      Data::Action(groups::mute(api, &group, &user, minutes).await?)
    }
    Command::Card { group, user, card } => {
      let (group, user) = (ctx.collection_ref(&group)?, ctx.user_ref(&user)?);
      Data::Action(groups::card(api, &group, &user, &card).await?)
    }
    Command::Call { action, params } => Data::Value(call(api, &action, &params).await?),
  })
}

/// Any action with parameters given as JSON.
async fn call(api: &Api, action: &str, params: &str) -> Result<Value> {
  let params: Value = serde_json::from_str(params)
    .map_err(|e| Error::input(format!("parameters are not JSON: {e}")))?;
  api.call(action, params).await
}
