//! Feishu commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Page, Result};

use crate::api::Api;
use crate::{chats, messages, pins};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Details of one chat
  Chat { chat: String },
  /// Chats visible to the bot whose name matches
  SearchChats {
    query: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Pin a message in its chat (or unpin it)
  Pin {
    message: String,
    #[arg(long)]
    undo: bool,
  },
  /// Pinned messages of a chat
  Pins {
    chat: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Forward a message to a chat or user
  Forward { message: String, to: String },
  /// Who has read a message the bot sent
  ReadBy {
    message: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Chat { chat } => {
      let chat = chats::chat(api, &ctx.collection_ref(&chat)?).await?;
      Data::Collections(Page::last(vec![chat]))
    }
    Command::SearchChats { query, page } => Data::Collections(
      page
        .collect(async |r| chats::search(api, &query, &r).await)
        .await?,
    ),
    Command::Pin { message, undo } => {
      Data::Action(pins::pin(api, &ctx.post_ref(&message)?, undo).await?)
    }
    Command::Pins { chat, page } => {
      let chat = ctx.collection_ref(&chat)?;
      Data::Posts(
        page
          .collect(async |r| pins::pins(api, &chat, &r).await)
          .await?,
      )
    }
    Command::Forward { message, to } => {
      let (message, to) = (ctx.post_ref(&message)?, ctx.collection_ref(&to)?);
      Data::Action(messages::forward(api, &message, &to).await?)
    }
    Command::ReadBy { message, page } => {
      let message = ctx.post_ref(&message)?;
      Data::Users(
        page
          .collect(async |r| messages::readers(api, &message, &r).await)
          .await?,
      )
    }
  })
}
