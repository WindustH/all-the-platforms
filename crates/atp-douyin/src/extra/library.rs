//! Your library beyond the shared commands: the mixes (合集), short dramas
//! (短剧) and music you saved, and follow requests.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::relations;
use crate::saves::{self, Saved};

#[derive(Debug, clap::Subcommand)]
pub enum LibraryCommand {
  #[command(flatten)]
  Saved(SavedCommand),
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

const EX_SAVE: &str = "\
Examples:
  atp douyin saved-mixes
  atp douyin save https://www.douyin.com/collection/7123456789012345678
  atp douyin save '#2' '#3' '#5' --undo

`save-drama` and `save-music` do the same for short dramas (after
`saved-dramas`) and music (after `saved-music`).
More: atp guide douyin save";
