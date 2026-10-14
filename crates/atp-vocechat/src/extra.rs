//! VoceChat-only commands.

use atp_core::cli::read_text;
use atp_core::{Data, Message, Result};

use crate::api::Api;
use crate::messages;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Send a message written in Markdown
  #[command(after_long_help = EX_MARKDOWN)]
  Markdown {
    /// Chat: `group:<gid>`, `user:<uid>`, a web app chat link or #N
    chat: String,
    /// Text; `-` reads it from stdin
    text: String,
    /// Answer this message (`CHAT/MID` or #N; users only)
    #[arg(long, value_name = "MESSAGE")]
    reply_to: Option<String>,
  },
}

const EX_MARKDOWN: &str = "\
Examples:
  atp vocechat markdown group:2 '**Release 1.4** is out'
  atp vocechat markdown user:5 - < notes.md
  atp vocechat markdown group:2 'Fixed in `main`' --reply-to group:2/381

More: atp guide vocechat markdown";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Markdown {
      chat,
      text,
      reply_to,
    } => {
      let message = Message {
        text: read_text(Some(text))?,
        reply_to: reply_to.map(|r| ctx.post_ref(&r)).transpose()?,
        files: Vec::new(),
      };
      let chat = ctx.collection_ref(&chat)?;
      Data::Action(messages::send(api, &chat, &message, true).await?)
    }
  })
}
