//! Feishu commands beyond the shared set.

use social_core::cli::PageArgs;
use social_core::{Data, Page, Result};

use crate::api::Api;
use crate::{chats, messages};

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
    Command::Chat { chat } => Data::Collections(Page::last(vec![
      chats::info(api, &ctx.collection_ref(&chat)?).await?,
    ])),
    Command::SearchChats { query, page } => Data::Collections(
      page
        .collect(async |r| chats::search(api, &query, &r).await)
        .await?,
    ),
    Command::Pin { message, undo } => {
      Data::Action(messages::pin(api, &ctx.post_ref(&message)?, undo).await?)
    }
    Command::Pins { chat, page } => {
      let chat = ctx.collection_ref(&chat)?;
      Data::Posts(
        page
          .collect(async |r| messages::pins(api, &chat, &r).await)
          .await?,
      )
    }
    Command::Forward { message, to } => Data::Action(
      messages::forward(api, &ctx.post_ref(&message)?, &ctx.collection_ref(&to)?).await?,
    ),
    Command::ReadBy { message, page } => {
      let message = ctx.post_ref(&message)?;
      Data::Users(
        page
          .collect(async |r| messages::read_users(api, &message, &r).await)
          .await?,
      )
    }
  })
}
