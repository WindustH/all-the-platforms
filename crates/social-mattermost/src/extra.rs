//! Mattermost-only commands: teams, one channel, pins, joining and leaving.

use social_core::{Data, Result};

use crate::api::Api;
use crate::{chats, messages};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Your teams
  Teams,
  /// About a channel: purpose, header, message count
  Channel { chat: String },
  /// Pinned posts of a channel
  Pins { chat: String },
  /// Pin a post, or unpin it with --undo
  Pin {
    post: String,
    #[arg(long)]
    undo: bool,
  },
  /// Join a channel
  Join { chat: String },
  /// Leave a channel
  Leave { chat: String },
}

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
