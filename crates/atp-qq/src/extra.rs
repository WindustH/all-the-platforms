//! QQ-bot-only commands.

use atp_core::{Data, Result};

use crate::api::Api;
use crate::{chats, messages};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// A server's (频道) details
  Server { server: String },
  /// A server's roles
  Roles { server: String },
  /// Open a 私信 session with a server member; prints its `dm:` chat
  OpenDm {
    /// The member's user id
    user: String,
    /// The server they share with the bot
    server: String,
  },
  /// Pinned messages of a sub-channel
  Pins { channel: String },
  /// Pin a sub-channel message (or unpin it with --undo)
  Pin {
    message: String,
    #[arg(long)]
    undo: bool,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Server { server } => {
      Data::Value(chats::server(api, &ctx.collection_ref(&server)?).await?)
    }
    Command::Roles { server } => {
      Data::Value(chats::roles(api, &ctx.collection_ref(&server)?).await?)
    }
    Command::OpenDm { user, server } => {
      Data::Action(chats::open_dm(api, &user, &ctx.collection_ref(&server)?).await?)
    }
    Command::Pins { channel } => {
      Data::Value(messages::pins(api, &ctx.collection_ref(&channel)?).await?)
    }
    Command::Pin { message, undo } => {
      let mut action = messages::pin(api, &ctx.post_ref(&message)?, undo).await?;
      action.target = message;
      Data::Action(action)
    }
  })
}
