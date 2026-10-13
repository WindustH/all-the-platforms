//! OneBot-only commands: group administration and any other action.

use atp_core::{Action, Data, Error, Result, Value, json};

use crate::api::Api;
use crate::refs::{self, Chat, num};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Remove a member from a group
  Kick {
    group: String,
    user: String,
    /// Also refuse their future join requests
    #[arg(long)]
    reject: bool,
  },
  /// Mute a group member for some minutes (0 lifts it)
  Mute {
    group: String,
    user: String,
    #[arg(long, default_value_t = 10)]
    minutes: u64,
  },
  /// Set a member's group card (nickname in the group); empty clears it
  Card {
    group: String,
    user: String,
    card: String,
  },
  /// Call any OneBot action with JSON parameters
  #[command(after_long_help = EX_CALL)]
  Call {
    action: String,
    /// Parameters as a JSON object
    #[arg(default_value = "{}")]
    params: String,
  },
}

const EX_CALL: &str = "\
Examples:
  atp onebot call get_status
  atp onebot call get_group_info '{\"group_id\": 123456}'
  atp onebot call send_like '{\"user_id\": 10001, \"times\": 1}'";

fn group(arg: &str) -> Result<String> {
  match refs::chat(arg)? {
    Chat::Group(g) => Ok(g),
    Chat::Private(_) => Err(Error::input("give a group (group:<number>)")),
  }
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Kick {
      group: g,
      user,
      reject,
    } => {
      let g = group(&ctx.collection_ref(&g)?)?;
      let u = refs::user(&ctx.user_ref(&user)?)?;
      api
        .call(
          "set_group_kick",
          json!({"group_id": num(&g), "user_id": num(&u), "reject_add_request": reject}),
        )
        .await?;
      Data::Action(Action::done("kick", format!("{u} from group:{g}")))
    }
    Command::Mute {
      group: g,
      user,
      minutes,
    } => {
      let g = group(&ctx.collection_ref(&g)?)?;
      let u = refs::user(&ctx.user_ref(&user)?)?;
      api
        .call(
          "set_group_ban",
          json!({"group_id": num(&g), "user_id": num(&u), "duration": minutes * 60}),
        )
        .await?;
      let what = if minutes == 0 { "unmute" } else { "mute" };
      Data::Action(Action::done(what, format!("{u} in group:{g}")))
    }
    Command::Card {
      group: g,
      user,
      card,
    } => {
      let g = group(&ctx.collection_ref(&g)?)?;
      let u = refs::user(&ctx.user_ref(&user)?)?;
      api
        .call(
          "set_group_card",
          json!({"group_id": num(&g), "user_id": num(&u), "card": card}),
        )
        .await?;
      Data::Action(Action::done("card", format!("{u} in group:{g}")))
    }
    Command::Call { action, params } => {
      let params: Value = serde_json::from_str(&params)
        .map_err(|e| Error::input(format!("parameters are not JSON: {e}")))?;
      Data::Value(api.call(&action, params).await?)
    }
  })
}
