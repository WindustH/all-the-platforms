//! Mattermost-only commands: teams, one channel, pins, joining and leaving.

use atp_core::{Data, Result};

use crate::api::Api;
use crate::{chats, messages};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Your teams
  Teams,
  /// About a channel: purpose, header, message count
  #[command(after_long_help = EX_CHANNEL)]
  Channel {
    /// Channel: id, link, `TEAM/NAME`, `~name`, `@user` or #N
    chat: String,
  },
  /// Pinned posts of a channel
  Pins {
    /// Channel: id, link, `TEAM/NAME`, `~name`, `@user` or #N
    chat: String,
  },
  /// Pin a post, or unpin it with --undo
  Pin {
    /// Post: permalink, id, `CHANNEL/POST` or #N
    post: String,
    /// Unpin it instead
    #[arg(long)]
    undo: bool,
  },
  /// Join a channel
  #[command(after_long_help = EX_JOIN)]
  Join {
    /// Channel: id, link, `TEAM/NAME`, `~name` or #N
    chat: String,
  },
  /// Leave a channel
  Leave {
    /// Channel: id, link, `TEAM/NAME`, `~name` or #N
    chat: String,
  },
}

const EX_CHANNEL: &str = "\
Examples:
  atp mattermost channel town-square
  atp mattermost channel myteam/off-topic
  atp mattermost channel https://chat.example.com/myteam/channels/off-topic

More: atp guide mattermost channel";

const EX_JOIN: &str = "\
Examples:
  atp mattermost join myteam/releases
  atp mattermost join '~releases'

More: atp guide mattermost join";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Teams => Data::Collections(chats::teams(api).await?),
    Command::Channel { chat } => {
      Data::Collections(chats::channel(api, &ctx.collection_ref(&chat)?).await?)
    }
    Command::Pins { chat } => Data::Posts(messages::pins(api, &ctx.collection_ref(&chat)?).await?),
    Command::Pin { post, undo } => {
      Data::Action(messages::pin(api, &ctx.post_ref(&post)?, undo).await?)
    }
    Command::Join { chat } => {
      Data::Action(chats::join(api, &ctx.collection_ref(&chat)?, false).await?)
    }
    Command::Leave { chat } => {
      Data::Action(chats::join(api, &ctx.collection_ref(&chat)?, true).await?)
    }
  })
}
