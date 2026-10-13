//! Your library: history and watch-later management, favorites folders,
//! follow groups and the blacklist.

use social_core::cli::PageArgs;
use social_core::{Ctx, Data, Result};

use super::{confirm, video_ref};
use crate::{folder, history, relation};

#[derive(Debug, clap::Subcommand)]
pub enum Library {
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

pub async fn run(ctx: &Ctx, command: Library) -> Result<Data> {
  use Library as L;
  Ok(match command {
    L::HistorySearch { keyword, page } => Data::Posts(
      page
        .collect_dated(async |r| history::search(ctx, &keyword, &r).await)
        .await?,
    ),
    L::HistoryDelete { video } => {
      Data::Action(history::delete(ctx, &video_ref(ctx, &video).await?).await?)
    }
    L::HistoryClear { yes } => {
      confirm(yes, "erase your whole watch history")?;
      Data::Action(history::clear(ctx).await?)
    }
    L::HistoryPause { undo } => Data::Action(history::pause(ctx, undo).await?),
    L::LaterAdd { video } => {
      Data::Action(history::later_add(ctx, &video_ref(ctx, &video).await?).await?)
    }
    L::LaterRemove { video } => {
      Data::Action(history::later_remove(ctx, &video_ref(ctx, &video).await?).await?)
    }
    L::LaterClear { watched, yes } => {
      if !watched {
        confirm(yes, "empty your whole watch-later list")?;
      }
      Data::Action(history::later_clear(ctx, watched).await?)
    }
    L::FolderCreate {
      title,
      intro,
      private,
    } => Data::Action(folder::create(ctx, &title, &intro, private).await?),
    L::FolderEdit {
      folder,
      title,
      intro,
      private,
      public,
    } => {
      let folder = ctx.collection_ref(&folder)?;
      let privacy = (private || public).then_some(private);
      Data::Action(folder::edit(ctx, &folder, title.as_deref(), intro.as_deref(), privacy).await?)
    }
    L::FolderDelete { folder, yes } => {
      let folder = ctx.collection_ref(&folder)?;
      confirm(yes, &format!("delete folder {folder} and its contents"))?;
      Data::Action(folder::delete(ctx, &folder).await?)
    }
    L::FolderClean { folder } => {
      Data::Action(folder::clean(ctx, &ctx.collection_ref(&folder)?).await?)
    }
    L::SavedFolders { page } => Data::Collections(
      page
        .collect(async |r| folder::collected(ctx, &r).await)
        .await?,
    ),
    L::Save { folder, undo } => {
      Data::Action(folder::save(ctx, &ctx.collection_ref(&folder)?, undo).await?)
    }
    L::Groups => Data::Collections(relation::groups(ctx).await?),
    L::Group { group, page } => {
      let group = ctx.collection_ref(&group)?;
      Data::Users(
        page
          .collect(async |r| relation::members(ctx, &group, &r).await)
          .await?,
      )
    }
    L::GroupCreate { name } => Data::Action(relation::create_group(ctx, &name).await?),
    L::GroupRename { group, name } => {
      let group = ctx.collection_ref(&group)?;
      Data::Action(relation::rename_group(ctx, &group, &name).await?)
    }
    L::GroupDelete { group, yes } => {
      let group = ctx.collection_ref(&group)?;
      confirm(yes, &format!("delete follow group {group}"))?;
      Data::Action(relation::delete_group(ctx, &group).await?)
    }
    L::GroupAssign { users, groups } => {
      let users = users
        .iter()
        .map(|u| ctx.user_ref(u))
        .collect::<Result<Vec<_>>>()?;
      let groups = groups
        .iter()
        .map(|g| ctx.collection_ref(g))
        .collect::<Result<Vec<_>>>()?;
      Data::Action(relation::assign(ctx, &users, &groups).await?)
    }
    L::Special { user, undo } => {
      Data::Action(relation::special(ctx, &ctx.user_ref(&user)?, undo).await?)
    }
    L::Blacklist { page } => Data::Users(
      page
        .collect(async |r| relation::blacklist(ctx, &r).await)
        .await?,
    ),
    L::Block { user, undo } => {
      Data::Action(relation::block(ctx, &ctx.user_ref(&user)?, undo).await?)
    }
    L::SharedFollowing { user, page } => {
      let user = ctx.user_ref(&user)?;
      Data::Users(
        page
          .collect(async |r| relation::shared(ctx, &user, &r).await)
          .await?,
      )
    }
  })
}
