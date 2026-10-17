//! Your library: searching and pausing the history, cleaning and saving
//! favorites folders, special follows and followings shared with others.
//! Creating and editing folders, follow groups, blocks, watch later and
//! deleting from the history are shared commands.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{folders, history, relations};

#[derive(Debug, clap::Subcommand)]
pub enum LibraryCommand {
  #[command(flatten)]
  History(HistoryCommand),
  #[command(flatten)]
  Folders(FolderCommand),
  #[command(flatten)]
  Follows(FollowCommand),
}

pub async fn run(api: &Api, command: LibraryCommand) -> Result<Data> {
  match command {
    LibraryCommand::History(c) => run_history(api, c).await,
    LibraryCommand::Folders(c) => run_folders(api, c).await,
    LibraryCommand::Follows(c) => run_follows(api, c).await,
  }
}

/// Searching the watch history and pausing it; deleting from it, clearing
/// it and watch later are shared commands.
#[derive(Debug, clap::Subcommand)]
pub enum HistoryCommand {
  /// Search your watch history
  HistorySearch {
    /// Words to find in the titles
    keyword: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Stop recording the watch history (--undo resumes)
  HistoryPause {
    /// Resume recording
    #[arg(long)]
    undo: bool,
  },
}

async fn run_history(api: &Api, command: HistoryCommand) -> Result<Data> {
  use HistoryCommand as L;
  Ok(match command {
    L::HistorySearch { keyword, page } => Data::Posts(
      page
        .collect_dated(async |r| history::search(api, &keyword, &r).await)
        .await?,
    ),
    L::HistoryPause { undo } => Data::Action(history::pause(api, undo).await?),
  })
}

/// Cleaning favorites folders, and folders saved from others.
#[derive(Debug, clap::Subcommand)]
pub enum FolderCommand {
  /// Remove videos that became unavailable from a folder
  FolderClean {
    /// Folder: id, favlist?fid= link or #N
    folder: String,
  },
  /// Folders and collections you saved from other people
  SavedFolders {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Save someone's folder or collection (合集), or drop it with --undo
  Save {
    /// Folder (id or favlist?fid= link), 合集 (space link or `MID/ID`), or #N
    folder: String,
    /// Drop it from your saved folders
    #[arg(long)]
    undo: bool,
  },
}

async fn run_folders(api: &Api, command: FolderCommand) -> Result<Data> {
  use FolderCommand as L;
  Ok(match command {
    L::FolderClean { folder } => {
      Data::Action(folders::clean(api, &api.ctx.collection_ref(&folder)?).await?)
    }
    L::SavedFolders { page } => Data::Collections(
      page
        .collect(async |r| folders::collected(api, &r).await)
        .await?,
    ),
    L::Save { folder, undo } => {
      Data::Action(folders::save(api, &api.ctx.collection_ref(&folder)?, undo).await?)
    }
  })
}

/// Special follows and followings shared with others.
#[derive(Debug, clap::Subcommand)]
pub enum FollowCommand {
  /// Mark a user as a special follow (特别关注), or unmark with --undo
  Special {
    /// User: mid, `uid:MID`, space link, name or #N
    user: String,
    /// Unmark the user
    #[arg(long)]
    undo: bool,
  },
  /// People both you and USER follow
  SharedFollowing {
    /// User: mid, `uid:MID`, space link, name or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

async fn run_follows(api: &Api, command: FollowCommand) -> Result<Data> {
  use FollowCommand as L;
  Ok(match command {
    L::Special { user, undo } => {
      Data::Action(relations::special(api, &api.ctx.user_ref(&user)?, undo).await?)
    }
    L::SharedFollowing { user, page } => {
      let user = api.ctx.user_ref(&user)?;
      Data::Users(
        page
          .collect(async |r| relations::shared(api, &user, &r).await)
          .await?,
      )
    }
  })
}
