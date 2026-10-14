//! QQ-bot-only commands.

use atp_core::{Data, Result};

use crate::api::Api;
use crate::{chats, messages};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// A server's (频道) details
  Server {
    /// Server: `guild:<id>` or #N
    server: String,
  },
  /// A server's roles
  Roles {
    /// Server: `guild:<id>` or #N
    server: String,
  },
  /// Open a 私信 session with a server member; prints its `dm:` chat
  #[command(after_long_help = EX_OPEN_DM)]
  OpenDm {
    /// The member's user id
    user: String,
    /// The server they share with the bot
    server: String,
  },
  /// Pinned messages of a sub-channel
  Pins {
    /// Sub-channel id
    channel: String,
  },
  /// Pin a sub-channel message (or unpin it with --undo)
  #[command(after_long_help = EX_PIN)]
  Pin {
    /// Message: `CHANNEL/MESSAGE`, as `listen` and `pins` print it
    message: String,
    /// Unpin it instead
    #[arg(long)]
    undo: bool,
  },
}

const EX_OPEN_DM: &str = "\
Examples:
  atp qq open-dm USER_ID guild:GUILD_ID      # prints dm:<session>
  atp qq send dm:SESSION \"hello\"

More: atp guide qq open-dm";

const EX_PIN: &str = "\
Examples:
  atp qq pins CHANNEL_ID
  atp qq pin CHANNEL_ID/MESSAGE_ID
  atp qq pin CHANNEL_ID/MESSAGE_ID --undo

More: atp guide qq pin";

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
