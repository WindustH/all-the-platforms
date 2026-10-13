//! Telegram-only commands.

use social_core::cli::PageArgs;
use social_core::{Data, Result};

use crate::refs::{self, Chat};
use crate::{Mode, Telegram, login, user, web};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Log in a user account with its phone number and the code Telegram sends
  LoginPhone {
    /// International format, e.g. +447700900123 (asked when left out)
    phone: Option<String>,
  },
  /// Search the messages of one chat (public channels need no account)
  SearchChat {
    chat: String,
    query: String,
    /// photo, video, media, document, url, voice, music, gif, round, mention
    #[arg(long)]
    filter: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Mark a chat as read
  MarkRead { chat: String },
  /// Pin a message (or unpin it with --undo)
  Pin {
    message: String,
    #[arg(long)]
    undo: bool,
  },
  /// Forward a message to another chat
  Forward { message: String, to: String },
}

pub async fn run(tg: &Telegram, command: Command) -> Result<Data> {
  let ctx = &tg.ctx;
  Ok(match command {
    Command::LoginPhone { phone } => {
      let me = login::phone(ctx, &tg.mt, phone).await?;
      Data::User(Box::new(me))
    }
    Command::SearchChat {
      chat,
      query,
      filter,
      page,
    } => {
      let chat = refs::chat(&ctx.collection_ref(&chat)?)?;
      let filter = filter.as_deref();
      let posts = match tg.mode()? {
        Mode::User(_) => {
          page
            .collect_dated(async |r| {
              tg.as_user(async |c| user::search_chat(c, &chat, &query, filter, &r).await)
                .await
            })
            .await?
        }
        _ => match &chat {
          Chat::Username(u) => {
            page
              .collect_dated(async |r| web::posts(ctx, u, Some(&query), r.cursor.as_deref()).await)
              .await?
          }
          _ => return Err(tg.needs_account("searching this chat")),
        },
      };
      Data::Posts(posts)
    }
    Command::MarkRead { chat } => {
      let chat = ctx.collection_ref(&chat)?;
      Data::Action(
        tg.as_user(async |c| user::mark_read(c, &chat).await)
          .await?,
      )
    }
    Command::Pin { message, undo } => {
      let message = ctx.post_ref(&message)?;
      Data::Action(match tg.mode()? {
        Mode::User(_) => {
          tg.as_user(async |c| user::pin(c, &message, undo).await)
            .await?
        }
        Mode::Bot(b) => b.pin(ctx, &message, undo).await?,
        Mode::Web => return Err(tg.needs_account("pinning")),
      })
    }
    Command::Forward { message, to } => {
      let message = ctx.post_ref(&message)?;
      let to = ctx.collection_ref(&to)?;
      Data::Action(match tg.mode()? {
        Mode::User(_) => {
          tg.as_user(async |c| user::forward(c, &message, &to).await)
            .await?
        }
        Mode::Bot(b) => b.forward(ctx, &message, &to).await?,
        Mode::Web => return Err(tg.needs_account("forwarding")),
      })
    }
  })
}
