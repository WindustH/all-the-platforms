//! Managing your lists: create, edit, delete, members, following.

use atp_core::{Data, Result};

use super::confirm;
use crate::api::Api;
use crate::lists;

// The variant names are the command names (`list-create` ...).
#[allow(clippy::enum_variant_names)]
#[derive(Debug, clap::Subcommand)]
pub enum OwnListCommand {
  /// Create a list
  ListCreate {
    name: String,
    #[arg(short, long)]
    description: Option<String>,
    #[arg(long)]
    private: bool,
  },
  /// Rename a list, change its description or privacy
  ListUpdate {
    list: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(short, long)]
    description: Option<String>,
    #[arg(long)]
    private: Option<bool>,
  },
  /// Delete one of your lists
  ListDelete {
    list: String,
    #[arg(short, long)]
    yes: bool,
  },
  /// Add a user to a list (or remove them with --undo)
  ListAdd {
    list: String,
    user: String,
    #[arg(long)]
    undo: bool,
  },
  /// Follow a list (or unfollow it with --undo)
  ListFollow {
    list: String,
    #[arg(long)]
    undo: bool,
  },
}

pub(super) async fn run(api: &Api, command: OwnListCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    OwnListCommand::ListCreate {
      name,
      description,
      private,
    } => Data::Action(lists::create(api, &name, description.as_deref(), private).await?),
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
    OwnListCommand::ListDelete { list, yes } => {
      confirm(yes, "delete the list")?;
      Data::Action(lists::delete(api, &ctx.collection_ref(&list)?).await?)
    }
    OwnListCommand::ListAdd { list, user, undo } => Data::Action(
      lists::member(
        api,
        &ctx.collection_ref(&list)?,
        &ctx.user_ref(&user)?,
        undo,
      )
      .await?,
    ),
    OwnListCommand::ListFollow { list, undo } => {
      Data::Action(lists::follow(api, &ctx.collection_ref(&list)?, undo).await?)
    }
  })
}
