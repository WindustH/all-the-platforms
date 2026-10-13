//! WeCom commands beyond the shared set.

use atp_core::{Data, Page, Result};

use crate::api::Api;
use crate::chats;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Departments (all, or those under one)
  Departments { parent: Option<String> },
  /// Details of an app group chat (chat:CHATID)
  Chat { chat: String },
  /// Create an app group chat with these users (at least two)
  CreateChat {
    name: String,
    #[arg(required = true, num_args = 2..)]
    users: Vec<String>,
    /// Owner (default: a random member)
    #[arg(long)]
    owner: Option<String>,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Departments { parent } => {
      Data::Collections(chats::departments(api, parent.as_deref()).await?)
    }
    Command::Chat { chat } => Data::Collections(Page::last(vec![
      chats::chat(api, &api.ctx.collection_ref(&chat)?).await?,
    ])),
    Command::CreateChat { name, users, owner } => Data::Collections(Page::last(vec![
      chats::create_chat(api, &name, owner.as_deref(), &users).await?,
    ])),
  })
}
