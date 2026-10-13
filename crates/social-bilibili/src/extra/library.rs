//! Your library: history and watch-later management, favorites folders,
//! follow groups and the blacklist.

use social_core::cli::PageArgs;
use social_core::{Data, Result};

use super::{confirm, video_ref};
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
    keyword: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Remove a video from your watch history
  HistoryDelete { video: String },
  /// Erase your whole watch history
  HistoryClear {
    #[arg(short, long)]
    yes: bool,
  },
  /// Stop recording the watch history (--undo resumes)
  HistoryPause {
    #[arg(long)]
    undo: bool,
  },
  /// Add a video to watch later
  LaterAdd { video: String },
  /// Remove a video from watch later
  LaterRemove { video: String },
  /// Empty watch later, or only the videos already watched
  LaterClear {
    #[arg(long)]
    watched: bool,
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
  FolderCreate {
    title: String,
    #[arg(long, default_value = "")]
    intro: String,
    #[arg(long)]
    private: bool,
  },
  /// Rename a favorites folder or change its description / visibility
  FolderEdit {
    folder: String,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    intro: Option<String>,
    #[arg(long, conflicts_with = "public")]
    private: bool,
    #[arg(long)]
    public: bool,
  },
  /// Delete a favorites folder and what it holds
  FolderDelete {
    folder: String,
    #[arg(short, long)]
    yes: bool,
  },
  /// Remove videos that became unavailable from a folder
  FolderClean { folder: String },
  /// Folders and collections you saved from other people
  SavedFolders {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Save someone's folder or collection (合集), or drop it with --undo
  Save {
    folder: String,
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
    group: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Create a follow group
  GroupCreate { name: String },
  /// Rename a follow group
  GroupRename { group: String, name: String },
  /// Delete a follow group (its people move to the default group)
  GroupDelete {
    group: String,
    #[arg(short, long)]
    yes: bool,
  },
  /// Put users into follow groups (replacing their groups; none = default)
  GroupAssign {
    #[arg(required = true)]
    users: Vec<String>,
    #[arg(long = "to", value_name = "GROUP")]
    groups: Vec<String>,
  },
  /// Mark a user as a special follow (特别关注), or unmark with --undo
  Special {
    user: String,
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
    user: String,
    #[arg(long)]
    undo: bool,
  },
  /// People both you and USER follow
  SharedFollowing {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

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
