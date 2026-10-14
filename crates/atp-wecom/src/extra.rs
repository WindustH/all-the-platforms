//! WeCom commands beyond the shared set.

use atp_core::{Data, Page, Result};

use crate::api::Api;
use crate::chats;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Departments (all, or those under one)
  Departments {
    /// Parent department: `party:ID` or its id
    parent: Option<String>,
  },
  /// Details of an app group chat (chat:CHATID)
  Chat {
    /// App group chat: `chat:CHATID` or #N
    chat: String,
  },
  /// Create an app group chat with these users (at least two)
  #[command(after_long_help = EX_CREATE_CHAT)]
  CreateChat {
    /// Name of the chat
    name: String,
    /// Members' user ids
    #[arg(required = true, num_args = 2..)]
    users: Vec<String>,
    /// Owner (default: a random member)
    #[arg(long)]
    owner: Option<String>,
  },
}

const EX_CREATE_CHAT: &str = "\
Examples:
  atp wecom create-chat \"Release\" zhangsan lisi wangwu --owner zhangsan
  atp wecom send chat:CHATID \"Build 42 is out\"

More: atp guide wecom create-chat";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Departments { parent } => {
      Data::Collections(chats::departments(api, parent.as_deref()).await?)
    }
    Command::Chat { chat } => {
      let chat = chats::chat(api, &api.ctx.collection_ref(&chat)?).await?;
      Data::Collections(Page::last(vec![chat]))
    }
    Command::CreateChat { name, users, owner } => {
      let chat = chats::create_chat(api, &name, owner.as_deref(), &users).await?;
      Data::Collections(Page::last(vec![chat]))
    }
  })
}
