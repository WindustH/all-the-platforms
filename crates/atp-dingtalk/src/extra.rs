//! DingTalk commands beyond the shared set.

use atp_core::{Data, Page, Result};

use crate::api::Api;
use crate::chats;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Details of a scene group (场景群) by openConversationId
  Group {
    /// The group's openConversationId (`cid…`)
    chat: String,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Group { chat } => {
      let group = chats::group(api, &api.ctx.collection_ref(&chat)?).await?;
      Data::Collections(Page::last(vec![group]))
    }
  })
}
