//! Chat commands: conversations, messages, sending and live events.

use std::path::PathBuf;

use clap::Subcommand;

use super::args::PageArgs;
use super::input::{check_files, read_text};
use crate::error::{Error, Result};
use crate::model::{Action, Data};
use crate::output::{Format, Stream};
use crate::platform::{Message, Platform};

#[derive(Debug, Subcommand)]
pub enum ChatCommand {
  /// Your conversations: servers, groups, channels and direct messages
  #[command(after_long_help = EX_CHATS)]
  Chats {
    /// Only the channels inside this server / team / space
    parent: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Messages of a chat, newest first
  Messages {
    chat: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Send a message to a chat
  Send {
    chat: String,
    /// Message text; `-` reads it from stdin
    #[arg(required_unless_present = "files")]
    text: Option<String>,
    /// Attach a file (repeatable)
    #[arg(short = 'a', long = "attach", value_name = "PATH")]
    files: Vec<PathBuf>,
    /// Answer this message
    #[arg(long, value_name = "MESSAGE")]
    reply_to: Option<String>,
  },
  /// Members of a chat, group or server
  Members {
    chat: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// React to a message or post with an emoji
  React {
    post: String,
    emoji: String,
    /// Remove the reaction instead
    #[arg(long)]
    undo: bool,
  },
  /// Replace the text of one of your messages
  Edit {
    post: String,
    /// New text; `-` reads it from stdin
    text: String,
  },
  /// Print messages as they arrive, until interrupted
  #[command(after_long_help = EX_LISTEN)]
  Listen {
    /// Only these chats (default: everything the account receives)
    chats: Vec<String>,
  },
}

/// `format` and `raw` shape the live output of `listen`.
pub(super) async fn run<P: Platform>(
  p: &P,
  cmd: ChatCommand,
  format: Format,
  raw: bool,
) -> Result<Data> {
  let ctx = p.ctx();
  Ok(match cmd {
    ChatCommand::Chats { parent, page } => {
      let parent = parent.map(|c| ctx.collection_ref(&c)).transpose()?;
      let parent = parent.as_deref();
      Data::Collections(page.collect(async |r| p.chats(parent, &r).await).await?)
    }
    ChatCommand::Messages { chat, page } => {
      let chat = ctx.collection_ref(&chat)?;
      Data::Posts(
        page
          .collect_dated(async |r| p.messages(&chat, &r).await)
          .await?,
      )
    }
    ChatCommand::Send {
      chat,
      text,
      files,
      reply_to,
    } => send(p, &chat, text, files, reply_to).await?,
    ChatCommand::Members { chat, page } => {
      let chat = ctx.collection_ref(&chat)?;
      Data::Users(page.collect(async |r| p.members(&chat, &r).await).await?)
    }
    ChatCommand::React { post, emoji, undo } => {
      Data::Action(p.react(&ctx.post_ref(&post)?, &emoji, undo).await?)
    }
    ChatCommand::Edit { post, text } => {
      let text = read_text(Some(text))?;
      if text.is_empty() {
        return Err(Error::input("the new text is empty"));
      }
      Data::Action(p.edit(&ctx.post_ref(&post)?, &text).await?)
    }
    ChatCommand::Listen { chats } => {
      listen(p, &chats, &Stream::new(format, P::INFO.id, raw)).await?
    }
  })
}

async fn send<P: Platform>(
  p: &P,
  chat: &str,
  text: Option<String>,
  files: Vec<PathBuf>,
  reply_to: Option<String>,
) -> Result<Data> {
  let ctx = p.ctx();
  check_files(&files)?;
  let message = Message {
    text: read_text(text)?,
    reply_to: reply_to.map(|r| ctx.post_ref(&r)).transpose()?,
    files,
  };
  if message.text.is_empty() && message.files.is_empty() {
    return Err(Error::input("nothing to send: give a text or --attach"));
  }
  Ok(Data::Action(
    p.send(&ctx.collection_ref(chat)?, &message).await?,
  ))
}

async fn listen<P: Platform>(p: &P, chats: &[String], out: &Stream) -> Result<Data> {
  let chats = chats
    .iter()
    .map(|c| p.ctx().collection_ref(c))
    .collect::<Result<Vec<_>>>()?;
  p.listen(&chats, out).await?;
  Ok(Data::Action(
    Action::done("listen", P::INFO.id).with_message("the stream ended"),
  ))
}

const EX_CHATS: &str = "\
Examples:
  atp discord chats                 # servers and direct messages
  atp discord chats '#2'            # the channels of the second server
  atp discord messages '#3' -n 200 --since 7d -f jsonl > channel.jsonl
  atp tg send '#1' 'Hello' -a photo.jpg

Chats are collections and messages are posts: `#N` works for both.";

const EX_LISTEN: &str = "\
Examples:
  atp tg listen                     # everything, one line per message
  atp discord listen '#3' -f jsonl  # one JSON object per event, as it arrives

Events other than new messages (edits, deletions, reactions ...) carry their
kind in `kind`.";
