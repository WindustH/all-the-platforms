//! DingTalk commands beyond the shared set.

use atp_core::{Data, Page, Result};

use crate::api::Api;
use crate::groups;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Details of a scene group (场景群) by openConversationId
  Group { chat: String },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Group { chat } => Data::Collections(Page::last(vec![
      groups::info(api, &api.ctx.collection_ref(&chat)?).await?,
    ])),
  })
}
