//! Bookmark folders.

use social_core::cli::PageArgs;
use social_core::{Data, Result};

use super::{confirm, one};
use crate::api::Api;
use crate::{account, users};

#[derive(Debug, clap::Subcommand)]
pub enum FolderCommand {
  /// Your bookmark folders (ids for `favorites --folder` and `favorite --folder`)
  Folders {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Create a bookmark folder
  FolderCreate { name: String },
  /// Rename a bookmark folder
  FolderRename { folder: String, name: String },
  /// Delete a bookmark folder (its bookmarks stay bookmarked)
  FolderDelete {
    folder: String,
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
