//! Managing your lists: create, edit, delete, members, following.

use atp_core::cli::confirm;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::lists;

const EX_LIST_UPDATE: &str = "\
Examples:
  atp x collections                    # your lists, numbered
  atp x list-update '#1' --name \"Space news\" -d \"Agencies and missions\"
  atp x list-update '#1' --private false";

const EX_LIST_ADD: &str = "\
Examples:
  atp x list-add '#1' NASA
  atp x list-add https://x.com/i/lists/1234567890123456789 @ESA --undo";

// The variant names are the command names (`list-create` ...).
#[allow(clippy::enum_variant_names)]
#[derive(Debug, clap::Subcommand)]
pub enum OwnListCommand {
  /// Create a list
  ListCreate {
    /// Name of the list
    name: String,
    /// Description of the list
    #[arg(short, long)]
    description: Option<String>,
    /// Make the list private
    #[arg(long)]
    private: bool,
  },
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
  /// Delete one of your lists
  ListDelete {
    /// List: id, link or #N
    list: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Add a user to a list (or remove them with --undo)
  #[command(after_long_help = EX_LIST_ADD)]
  ListAdd {
    /// List: id, link or #N
    list: String,
    /// User: handle, @handle, link, id or #N
    user: String,
    /// Remove the user instead
    #[arg(long)]
    undo: bool,
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
      let list = ctx.collection_ref(&list)?;
      confirm(yes, &format!("delete list {list}"))?;
      Data::Action(lists::delete(api, &list).await?)
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
