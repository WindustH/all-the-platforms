//! Your library: history and watch-later management, favorites folders,
//! follow groups and the blacklist.

use atp_core::cli::{PageArgs, confirm};
use atp_core::{Data, Result};

use super::video_ref;
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

/// Watch history and watch later.
#[derive(Debug, clap::Subcommand)]
pub enum HistoryCommand {
  /// Search your watch history
  HistorySearch {
    /// Words to find in the titles
    keyword: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Remove a video from your watch history
  HistoryDelete {
    /// Video: BV… or av… id, link, b23.tv short link or #N
    video: String,
  },
  /// Erase your whole watch history
  HistoryClear {
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Stop recording the watch history (--undo resumes)
  HistoryPause {
    /// Resume recording
    #[arg(long)]
    undo: bool,
  },
  /// Add a video to watch later
  LaterAdd {
    /// Video: BV… or av… id, link, b23.tv short link or #N
    video: String,
  },
  /// Remove a video from watch later
  LaterRemove {
    /// Video: BV… or av… id, link, b23.tv short link or #N
    video: String,
  },
  /// Empty watch later, or only the videos already watched
  LaterClear {
    /// Only remove the videos already watched (no confirmation needed)
    #[arg(long)]
    watched: bool,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
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
    L::HistoryDelete { video } => {
      Data::Action(history::delete(api, &video_ref(api, &video).await?).await?)
    }
    L::HistoryClear { yes } => {
      confirm(yes, "erase your whole watch history")?;
      Data::Action(history::clear(api).await?)
    }
    L::HistoryPause { undo } => Data::Action(history::pause(api, undo).await?),
    L::LaterAdd { video } => {
      Data::Action(history::later_add(api, &video_ref(api, &video).await?).await?)
    }
    L::LaterRemove { video } => {
      Data::Action(history::later_remove(api, &video_ref(api, &video).await?).await?)
    }
    L::LaterClear { watched, yes } => {
      if !watched {
        confirm(yes, "empty your whole watch-later list")?;
      }
      Data::Action(history::later_clear(api, watched).await?)
    }
  })
}

/// Favorites folders, and folders saved from others.
#[derive(Debug, clap::Subcommand)]
pub enum FolderCommand {
  /// Create a favorites folder
  #[command(after_long_help = EX_FOLDER_CREATE)]
  FolderCreate {
    /// Folder name
    title: String,
    /// Description
    #[arg(long, default_value = "")]
    intro: String,
    /// Make the folder private
    #[arg(long)]
    private: bool,
  },
  /// Rename a favorites folder or change its description / visibility
  #[command(after_long_help = EX_FOLDER_EDIT)]
  FolderEdit {
    /// Folder: id, favlist?fid= link or #N
    folder: String,
    /// New name
    #[arg(long)]
    title: Option<String>,
    /// New description
    #[arg(long)]
    intro: Option<String>,
    /// Make the folder private
    #[arg(long, conflicts_with = "public")]
    private: bool,
    /// Make the folder public
    #[arg(long)]
    public: bool,
  },
  /// Delete a favorites folder and what it holds
  FolderDelete {
    /// Folder: id, favlist?fid= link or #N
    folder: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
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
    L::FolderCreate {
      title,
      intro,
      private,
    } => Data::Action(folders::create(api, &title, &intro, private).await?),
    L::FolderEdit {
      folder,
      title,
      intro,
      private,
      public,
    } => {
      let folder = api.ctx.collection_ref(&folder)?;
      let privacy = (private || public).then_some(private);
      Data::Action(folders::edit(api, &folder, title.as_deref(), intro.as_deref(), privacy).await?)
    }
    L::FolderDelete { folder, yes } => {
      let folder = api.ctx.collection_ref(&folder)?;
      confirm(yes, &format!("delete folder {folder} and its contents"))?;
      Data::Action(folders::delete(api, &folder).await?)
    }
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

/// Follow groups, special follows and the blacklist.
#[derive(Debug, clap::Subcommand)]
pub enum FollowCommand {
  /// Your follow groups (分组)
  Groups,
  /// The people in one of your follow groups (id or exact name)
  Group {
    /// Follow group: id, exact name or #N
    group: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Create a follow group
  GroupCreate {
    /// Group name
    name: String,
  },
  /// Rename a follow group
  GroupRename {
    /// Follow group: id, exact name or #N
    group: String,
    /// New name
    name: String,
  },
  /// Delete a follow group (its people move to the default group)
  GroupDelete {
    /// Follow group: id, exact name or #N
    group: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Put users into follow groups (replacing their groups; none = default)
  #[command(after_long_help = EX_GROUP_ASSIGN)]
  GroupAssign {
    /// Users: mids, `uid:MID`, space links, names or #N
    #[arg(required = true)]
    users: Vec<String>,
    /// Groups to put them in (id, exact name or #N; repeat for several);
    /// none moves them to the default group
    #[arg(long = "to", value_name = "GROUP")]
    groups: Vec<String>,
  },
  /// Mark a user as a special follow (特别关注), or unmark with --undo
  Special {
    /// User: mid, `uid:MID`, space link, name or #N
    user: String,
    /// Unmark the user
    #[arg(long)]
    undo: bool,
  },
  /// Users you blocked
  Blacklist {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Block a user, or unblock with --undo
  Block {
    /// User: mid, `uid:MID`, space link, name or #N
    user: String,
    /// Unblock the user
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

const EX_FOLDER_CREATE: &str = "\
Examples:
  atp bili folder-create 'Watch again'
  atp bili folder-create Drafts --intro 'for later' --private";

const EX_FOLDER_EDIT: &str = "\
Examples:
  atp bili collections
  atp bili folder-edit '#2' --title 'Music videos' --public
  atp bili folder-edit 1052622027 --intro ''";

const EX_GROUP_ASSIGN: &str = "\
Examples:
  atp bili group-assign 946974 --to Friends
  atp bili group-assign 946974 '#3' --to Friends --to 'Game streams'
  atp bili group-assign 946974

More: atp guide bili group-assign";

async fn run_follows(api: &Api, command: FollowCommand) -> Result<Data> {
  use FollowCommand as L;
  Ok(match command {
    L::Groups => Data::Collections(relations::groups(api).await?),
    L::Group { group, page } => {
      let group = api.ctx.collection_ref(&group)?;
      Data::Users(
        page
          .collect(async |r| relations::members(api, &group, &r).await)
          .await?,
      )
    }
    L::GroupCreate { name } => Data::Action(relations::create_group(api, &name).await?),
    L::GroupRename { group, name } => {
      let group = api.ctx.collection_ref(&group)?;
      Data::Action(relations::rename_group(api, &group, &name).await?)
    }
    L::GroupDelete { group, yes } => {
      let group = api.ctx.collection_ref(&group)?;
      confirm(yes, &format!("delete follow group {group}"))?;
      Data::Action(relations::delete_group(api, &group).await?)
    }
    L::GroupAssign { users, groups } => {
      let users = users
        .iter()
        .map(|u| api.ctx.user_ref(u))
        .collect::<Result<Vec<_>>>()?;
      let groups = groups
        .iter()
        .map(|g| api.ctx.collection_ref(g))
        .collect::<Result<Vec<_>>>()?;
      Data::Action(relations::assign(api, &users, &groups).await?)
    }
    L::Special { user, undo } => {
      Data::Action(relations::special(api, &api.ctx.user_ref(&user)?, undo).await?)
    }
    L::Blacklist { page } => Data::Users(
      page
        .collect(async |r| relations::blacklist(api, &r).await)
        .await?,
    ),
    L::Block { user, undo } => {
      Data::Action(relations::block(api, &api.ctx.user_ref(&user)?, undo).await?)
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
