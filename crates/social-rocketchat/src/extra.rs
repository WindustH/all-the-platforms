//! Rocket.Chat-only commands: one room, pins, joining and leaving.

use social_core::{Action, Data, Page, Result, ValueExt, json};

use crate::api::Api;
use crate::{messages, parse, rooms};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// About a room: topic, members, message count
  Room { room: String },
  /// Pinned messages of a room
  Pins { room: String },
  /// Pin a message, or unpin it with --undo
  Pin {
    message: String,
    #[arg(long)]
    undo: bool,
  },
  /// Join a public channel
  Join { room: String },
  /// Leave a room
  Leave { room: String },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Room { room } => {
      let rid = rooms::id(api, &ctx.collection_ref(&room)?).await?;
      let v = api.get("rooms.info", &[("roomId", rid)]).await?;
      let c = rooms::render(api, |view| parse::room(v.at("room"), view))?;
      Data::Collections(Page::last(vec![c]))
    }
    Command::Pins { room } => Data::Posts(messages::pins(api, &ctx.collection_ref(&room)?).await?),
    Command::Pin { message, undo } => {
      Data::Action(messages::pin(api, &ctx.post_ref(&message)?, undo).await?)
    }
    Command::Join { room } => {
      let rid = rooms::id(api, &ctx.collection_ref(&room)?).await?;
      api.post("channels.join", json!({"roomId": rid})).await?;
      Data::Action(Action::done("join", rid))
    }
    Command::Leave { room } => {
      let rid = rooms::id(api, &ctx.collection_ref(&room)?).await?;
      let info = rooms::info(api, &rid).await?;
      api
        .post(
          &format!("{}.leave", rooms::family(&info.t)),
          json!({"roomId": rid}),
        )
        .await?;
      Data::Action(Action::done("leave", rid))
    }
  })
}
