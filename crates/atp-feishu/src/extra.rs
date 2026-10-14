//! Feishu commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Page, Result};

use crate::api::Api;
use crate::{chats, messages, pins};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Details of one chat
  Chat {
    /// Chat: `oc_…` or #N
    chat: String,
  },
  /// Chats visible to the bot whose name matches
  SearchChats {
    /// Words of the chat's name
    query: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Pin a message in its chat (or unpin it)
  Pin {
    /// Message: `om_…` or #N
    message: String,
    /// Unpin it instead
    #[arg(long)]
    undo: bool,
  },
  /// Pinned messages of a chat
  Pins {
    /// Chat: `oc_…` or #N
    chat: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Forward a message to a chat or user
  #[command(after_long_help = EX_FORWARD)]
  Forward {
    /// Message: `om_…` or #N
    message: String,
    /// Chat (`oc_…`) or user (`ou_…`, `on_…`, an email or a user id)
    to: String,
  },
  /// Who has read a message the bot sent
  ReadBy {
    /// Message: `om_…` or #N
    message: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

const EX_FORWARD: &str = "\
Examples:
  atp feishu forward om_MESSAGE oc_CHAT
  atp feishu forward om_MESSAGE someone@example.com
  atp feishu forward '#3' ou_USER        # after `messages`

More: atp guide feishu forward";

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
