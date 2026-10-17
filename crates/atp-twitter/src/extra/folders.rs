//! Bookmark folders, and deleting every bookmark. Creating, renaming and
//! deleting folders and filing bookmarks are the shared `folder-*` commands.

use atp_core::cli::{PageArgs, confirm};
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{account, users};

const EX_FOLDERS: &str = "\
Examples:
  atp x folders                        # numbered
  atp x favorites --folder '#1'
  atp x favorite 1849213456789012345   # bookmark it first
  atp x folder-add '#1' 1849213456789012345
  atp x folder-create Reading

More: atp guide x favorites";

#[derive(Debug, clap::Subcommand)]
pub enum FolderCommand {
  /// Your bookmark folders (ids for `favorites --folder` and the `folder-*` commands)
  #[command(after_long_help = EX_FOLDERS)]
  Folders {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Delete all your bookmarks, in every folder
  BookmarksClear {
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
}

pub(super) async fn run(api: &Api, command: FolderCommand) -> Result<Data> {
  Ok(match command {
    FolderCommand::Folders { page } => Data::Collections(
      page
        .collect(async |r| users::folders(api, &r).await)
        .await?,
    ),
    FolderCommand::BookmarksClear { yes } => {
      confirm(yes, "delete all your bookmarks")?;
      Data::Action(account::clear_bookmarks(api).await?)
    }
  })
}
