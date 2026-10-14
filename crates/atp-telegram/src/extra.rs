//! Telegram-only commands.

use atp_core::cli::PageArgs;
use atp_core::{Data, Page, Post, Result};

use crate::api::{Api, Mode};
use crate::refs::{self, Chat};
use crate::{login, messages, mt, web};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Log in a user account with its phone number and the code Telegram sends
  #[command(after_long_help = EX_LOGIN_PHONE)]
  LoginPhone {
    /// International format, e.g. +447700900123 (asked when left out)
    phone: Option<String>,
  },
  /// Search the messages of one chat (public channels need no account)
  #[command(after_long_help = EX_SEARCH_CHAT)]
  SearchChat {
    /// Chat: @name, t.me link, numeric id or #N
    chat: String,
    /// Words to find
    query: String,
    /// photo, video, media, document, url, voice, music, gif, round, mention
    #[arg(long)]
    filter: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Mark a chat as read
  MarkRead {
    /// Chat: @name, t.me link, numeric id or #N
    chat: String,
  },
  /// Pin a message (or unpin it with --undo)
  Pin {
    /// Message: t.me link, `CHAT/ID` or #N
    message: String,
    /// Unpin it instead
    #[arg(long)]
    undo: bool,
  },
  /// Forward a message to another chat
  #[command(after_long_help = EX_FORWARD)]
  Forward {
    /// Message: t.me link, `CHAT/ID` or #N
    message: String,
    /// Chat to forward it to: @name, t.me link, numeric id or #N
    to: String,
  },
}

const EX_LOGIN_PHONE: &str = "\
Examples:
  atp telegram login-phone
  atp telegram login-phone +447700900123

More: atp guide telegram login";

const EX_SEARCH_CHAT: &str = "\
Examples:
  atp telegram search-chat @durov update
  atp telegram search-chat @mychannel invoice --filter document

More: atp guide telegram search-chat";

const EX_FORWARD: &str = "\
Examples:
  atp telegram forward https://t.me/durov/42 @friend
  atp telegram forward @mychannel/42 -1001234567890

More: atp guide telegram forward";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::LoginPhone { phone } => Data::User(Box::new(login::phone(api, phone).await?)),
    Command::SearchChat {
      chat,
      query,
      filter,
      page,
    } => {
      let chat = refs::chat(&ctx.collection_ref(&chat)?)?;
      Data::Posts(search_chat(api, &chat, &query, filter.as_deref(), page).await?)
    }
    Command::MarkRead { chat } => {
      let chat = ctx.collection_ref(&chat)?;
      Data::Action(api.as_user(async |c| mt::mark_read(c, &chat).await).await?)
    }
    Command::Pin { message, undo } => {
      Data::Action(messages::pin(api, &ctx.post_ref(&message)?, undo).await?)
    }
    Command::Forward { message, to } => {
      let (message, to) = (ctx.post_ref(&message)?, ctx.collection_ref(&to)?);
      Data::Action(messages::forward(api, &message, &to).await?)
    }
  })
}

/// One chat's messages matching `query`: through the account, or (public
/// channels) the web preview.
async fn search_chat(
  api: &Api,
  chat: &Chat,
  query: &str,
  filter: Option<&str>,
  page: PageArgs,
) -> Result<Page<Post>> {
  match (api.mode()?, chat) {
    (Mode::User(_), _) => {
      page
        .collect_dated(async |r| {
          api
            .as_user(async |c| mt::search_chat(c, chat, query, filter, &r).await)
            .await
        })
        .await
    }
    (_, Chat::Username(u)) => {
      page
        .collect_dated(async |r| web::posts(&api.ctx, u, Some(query), r.cursor.as_deref()).await)
        .await
    }
    _ => Err(api.needs_account("searching this chat")),
  }
}
