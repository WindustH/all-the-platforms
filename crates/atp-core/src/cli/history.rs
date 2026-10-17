//! What you watched and what you keep for later: removing entries of the
//! viewing history or all of it, and the watch-later list.

use clap::Subcommand;

use super::args::PageArgs;
use super::follows::several;
use super::input::confirm;
use crate::error::{Error, Result};
use crate::model::Data;
use crate::platform::{Cap, Platform};

#[derive(Debug, Subcommand)]
pub enum HistoryCommand {
  /// Remove posts from your viewing history
  #[command(after_long_help = EX_HISTORY_DELETE)]
  HistoryDelete {
    /// Posts: ids, links, or #N from the last list
    #[arg(required = true, value_name = "POST")]
    posts: Vec<String>,
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
  /// Your watch-later list
  #[command(after_long_help = EX_WATCH_LATER)]
  WatchLater {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Add posts to watch later
  #[command(after_long_help = EX_WATCH_LATER)]
  LaterAdd {
    /// Posts: ids, links, or #N from the last list
    #[arg(required = true, value_name = "POST")]
    posts: Vec<String>,
  },
  /// Remove posts from watch later
  LaterRemove {
    /// Posts: ids, links, or #N from the last list
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
}

pub(super) async fn run<P: Platform>(p: &P, cmd: HistoryCommand) -> Result<Data> {
  let ctx = p.ctx();
  let posts =
    |list: &[String]| -> Result<Vec<String>> { list.iter().map(|x| ctx.post_ref(x)).collect() };
  Ok(match cmd {
    HistoryCommand::HistoryDelete { posts: list, yes } => {
      let list = posts(&list)?;
      let what = format!("remove {} from your history", several("post", &list));
      confirm(yes, &what)?;
      Data::actions(p.history_delete(&list).await?)
    }
    HistoryCommand::HistoryClear { yes } => {
      confirm(yes, "erase your whole history")?;
      Data::Action(p.history_clear().await?)
    }
    HistoryCommand::WatchLater { page } => Data::Posts(
      page
        .collect_dated(async |r| p.watch_later(&r).await)
        .await?,
    ),
    HistoryCommand::LaterAdd { posts: list } => Data::actions(p.later_add(&posts(&list)?).await?),
    HistoryCommand::LaterRemove { posts: list } => {
      Data::actions(p.later_remove(&posts(&list)?).await?)
    }
    HistoryCommand::LaterClear { watched, yes } => {
      if watched && !P::INFO.supports(Cap::LaterClearWatched) {
        return Err(Error::unsupported("later-clear --watched"));
      }
      if !watched {
        confirm(yes, "empty your whole watch-later list")?;
      }
      Data::Action(p.later_clear(watched).await?)
    }
  })
}

const EX_HISTORY_DELETE: &str = "\
Examples:
  atp bili history -n 30
  atp bili history-delete '#2' '#5'
  atp xhs history-delete 6650a1b2000000001e03c7d4 -y
  atp douyin history-clear -y

More: atp guide <platform> history-delete";

const EX_WATCH_LATER: &str = "\
Examples:
  atp bili later-add BV1xx411c7mD '#3'
  atp bili watch-later -n 50
  atp douyin later-remove '#2' '#4'
  atp douyin later-clear --watched

More: atp guide <platform> watch-later";
