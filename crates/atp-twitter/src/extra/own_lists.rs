//! Your lists beyond the shared follow-group commands: their description
//! and privacy, and following others' lists.

use atp_core::{Data, Result};

use crate::api::Api;
use crate::lists;

const EX_LIST_UPDATE: &str = "\
Examples:
  atp x follow-groups                  # your lists, numbered
  atp x list-update '#1' --name \"Space news\" -d \"Agencies and missions\"
  atp x list-update '#1' --private false";

#[derive(Debug, clap::Subcommand)]
pub enum OwnListCommand {
  /// Rename a list, change its description or privacy
  #[command(after_long_help = EX_LIST_UPDATE)]
  ListUpdate {
    /// List: id, link or #N
    list: String,
    /// New name
    #[arg(long)]
    name: Option<String>,
    /// New description
    #[arg(short, long)]
    description: Option<String>,
    /// true makes the list private, false public
    #[arg(long)]
    private: Option<bool>,
  },
  /// Follow a list (or unfollow it with --undo)
  ListFollow {
    /// List: id, link or #N
    list: String,
    /// Unfollow instead
    #[arg(long)]
    undo: bool,
  },
}

pub(super) async fn run(api: &Api, command: OwnListCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    OwnListCommand::ListUpdate {
      list,
      name,
      description,
      private,
    } => Data::Action(
      lists::update(
        api,
        &ctx.collection_ref(&list)?,
        name.as_deref(),
        description.as_deref(),
        private,
      )
      .await?,
    ),
    OwnListCommand::ListFollow { list, undo } => {
      Data::Action(lists::follow(api, &ctx.collection_ref(&list)?, undo).await?)
    }
  })
}
