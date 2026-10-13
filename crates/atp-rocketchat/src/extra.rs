//! Rocket.Chat-only commands: one room, pins, joining and leaving.

use atp_core::{Data, Result};

use crate::api::Api;
use crate::{chats, messages};

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
      Data::Collections(chats::room(api, &ctx.collection_ref(&room)?).await?)
    }
    Command::Pins { room } => Data::Posts(messages::pins(api, &ctx.collection_ref(&room)?).await?),
    Command::Pin { message, undo } => {
      Data::Action(messages::pin(api, &ctx.post_ref(&message)?, undo).await?)
    }
    Command::Join { room } => {
      Data::Action(chats::join(api, &ctx.collection_ref(&room)?, false).await?)
    }
    Command::Leave { room } => {
      Data::Action(chats::join(api, &ctx.collection_ref(&room)?, true).await?)
    }
  })
}
