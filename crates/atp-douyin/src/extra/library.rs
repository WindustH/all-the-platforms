//! Your library beyond the shared commands: the mixes (合集), short dramas
//! (短剧) and music you saved, watch later (稍后再看), clearing the
//! history, and follow requests.

use atp_core::cli::{PageArgs, confirm};
use atp_core::{Data, Result};

use crate::api::Api;
use crate::saves::{self, Saved};
use crate::{history, relations};

#[derive(Debug, clap::Subcommand)]
pub enum LibraryCommand {
  #[command(flatten)]
  Saved(SavedCommand),
  #[command(flatten)]
  Later(LaterCommand),
  /// Approve requests to follow your private account (关注请求)
  ApproveFollower {
    /// Users: sec_uid, douyin.com/user/… link, @douyin_id or #N
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
  },
}

pub async fn run(api: &Api, command: LibraryCommand) -> Result<Data> {
  match command {
    LibraryCommand::Saved(c) => run_saved(api, c).await,
    LibraryCommand::Later(c) => run_later(api, c).await,
    LibraryCommand::ApproveFollower { users } => {
      let users = users
        .iter()
        .map(|u| api.ctx.user_ref(u))
        .collect::<Result<Vec<_>>>()?;
      Ok(Data::actions(relations::approve(api, &users).await?))
    }
  }
}

/// The 合集, 短剧 and 音乐 tabs of 收藏.
#[derive(Debug, clap::Subcommand)]
pub enum SavedCommand {
  /// Mixes (合集) you saved
  SavedMixes {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Short dramas (短剧) you saved
  SavedDramas {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Music you saved
  SavedMusic {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Save mixes (合集), or unsave them with --undo
  #[command(after_long_help = EX_SAVE)]
  Save {
    /// Mixes: id, douyin.com/collection/… link or #N
    #[arg(required = true, value_name = "MIX")]
    mixes: Vec<String>,
    /// Unsave them
    #[arg(long)]
    undo: bool,
  },
  /// Save short dramas (短剧), or unsave them with --undo
  SaveDrama {
    /// Short dramas: id or #N after `saved-dramas`
    #[arg(required = true, value_name = "DRAMA")]
    dramas: Vec<String>,
    /// Unsave them
    #[arg(long)]
    undo: bool,
  },
  /// Save music, or unsave it with --undo
  SaveMusic {
    /// Music: id, douyin.com/music/… link or #N
    #[arg(required = true, value_name = "MUSIC")]
    music: Vec<String>,
    /// Unsave it
    #[arg(long)]
    undo: bool,
  },
}

async fn run_saved(api: &Api, command: SavedCommand) -> Result<Data> {
  use SavedCommand as S;
  let listing = async |kind, page: PageArgs| -> Result<Data> {
    Ok(Data::Collections(
      page
        .collect(async |r| saves::list(api, kind, &r).await)
        .await?,
    ))
  };
  let save = async |kind, items: Vec<String>, undo| -> Result<Data> {
    let items = items
      .iter()
      .map(|i| api.ctx.collection_ref(i))
      .collect::<Result<Vec<_>>>()?;
    Ok(Data::actions(saves::save(api, kind, &items, undo).await?))
  };
  match command {
    S::SavedMixes { page } => listing(Saved::Mix, page).await,
    S::SavedDramas { page } => listing(Saved::Drama, page).await,
    S::SavedMusic { page } => listing(Saved::Music, page).await,
    S::Save { mixes, undo } => save(Saved::Mix, mixes, undo).await,
    S::SaveDrama { dramas, undo } => save(Saved::Drama, dramas, undo).await,
    S::SaveMusic { music, undo } => save(Saved::Music, music, undo).await,
  }
}

/// Watch later and the viewing history.
#[derive(Debug, clap::Subcommand)]
pub enum LaterCommand {
  /// Your watch-later list (稍后再看)
  WatchLater {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Add posts to watch later
  #[command(after_long_help = EX_LATER)]
  LaterAdd {
    /// Posts: aweme id, douyin.com link, v.douyin.com short link or #N
    #[arg(required = true, value_name = "POST")]
    posts: Vec<String>,
  },
  /// Remove posts from watch later
  LaterRemove {
    /// Posts: aweme id, douyin.com link, v.douyin.com short link or #N
    #[arg(required = true, value_name = "POST")]
    posts: Vec<String>,
  },
  /// Empty watch later, or only the posts already watched
  LaterClear {
    /// Only remove the posts already watched (no confirmation needed)
    #[arg(long)]
    watched: bool,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Erase your whole viewing history
  HistoryClear {
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
}

async fn run_later(api: &Api, command: LaterCommand) -> Result<Data> {
  use LaterCommand as L;
  let posts =
    |list: &[String]| -> Result<Vec<String>> { list.iter().map(|p| api.ctx.post_ref(p)).collect() };
  Ok(match command {
    L::WatchLater { page } => Data::Posts(
      page
        .collect(async |r| history::later(api, &r).await)
        .await?,
    ),
    L::LaterAdd { posts: list } => Data::actions(history::later_add(api, &posts(&list)?).await?),
    L::LaterRemove { posts: list } => {
      Data::actions(history::later_remove(api, &posts(&list)?).await?)
    }
    L::LaterClear { watched, yes } => {
      if !watched {
        confirm(yes, "empty your whole watch-later list")?;
      }
      Data::Action(history::later_clear(api, watched).await?)
    }
    L::HistoryClear { yes } => {
      confirm(yes, "erase your whole viewing history")?;
      Data::Action(history::clear(api).await?)
    }
  })
}

const EX_SAVE: &str = "\
Examples:
  atp douyin saved-mixes
  atp douyin save https://www.douyin.com/collection/7123456789012345678
  atp douyin save '#2' '#3' '#5' --undo

`save-drama` and `save-music` do the same for short dramas (after
`saved-dramas`) and music (after `saved-music`).
More: atp guide douyin save";

const EX_LATER: &str = "\
Examples:
  atp douyin later-add '#1' '#4'
  atp douyin watch-later
  atp douyin later-remove '#2'
  atp douyin later-clear --watched

More: atp guide douyin watch-later";
