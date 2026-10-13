//! VoceChat-only commands.

use atp_core::cli::read_text;
use atp_core::{Data, Message, Result};

use crate::api::Api;
use crate::messages;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Send a message written in Markdown
  Markdown {
    chat: String,
    /// Text; `-` reads it from stdin
    text: String,
    #[arg(long, value_name = "MESSAGE")]
    reply_to: Option<String>,
  },
}

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
