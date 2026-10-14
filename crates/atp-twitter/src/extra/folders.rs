//! Bookmark folders.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use super::{confirm, one};
use crate::api::Api;
use crate::{account, users};

const EX_FOLDERS: &str = "\
Examples:
  atp x folders                        # numbered
  atp x favorites --folder '#1'
  atp x favorite 1849213456789012345   # bookmark it first
  atp x favorite 1849213456789012345 --folder '#1'

More: atp guide x favorites";

#[derive(Debug, clap::Subcommand)]
pub enum FolderCommand {
  /// Your bookmark folders (ids for `favorites --folder` and `favorite --folder`)
  #[command(after_long_help = EX_FOLDERS)]
  Folders {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Create a bookmark folder
  FolderCreate {
    /// Name of the folder
    name: String,
  },
  /// Rename a bookmark folder
  FolderRename {
    /// Folder: id or #N, from `folders`
    folder: String,
    /// New name
    name: String,
  },
  /// Delete a bookmark folder (its bookmarks stay bookmarked)
  FolderDelete {
    /// Folder: id or #N, from `folders`
    folder: String,
    /// Confirm the deletion (required)
    #[arg(short, long)]
    yes: bool,
  },
}

pub(super) async fn run(api: &Api, command: FolderCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    FolderCommand::Folders { page } => Data::Collections(
      page
        .collect(async |r| users::folders(api, &r).await)
        .await?,
    ),
    FolderCommand::FolderCreate { name } => one(account::create_folder(api, &name).await?),
    FolderCommand::FolderRename { folder, name } => {
      Data::Action(account::rename_folder(api, &ctx.collection_ref(&folder)?, &name).await?)
    }
    FolderCommand::FolderDelete { folder, yes } => {
      confirm(yes, "delete the folder")?;
      Data::Action(account::delete_folder(api, &ctx.collection_ref(&folder)?).await?)
    }
  })
}
