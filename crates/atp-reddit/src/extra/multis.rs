//! Multireddits.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};
use clap::builder::PossibleValuesParser;

use super::SORTS;
use crate::api::Api;
use crate::multis;
use crate::posts::TIME_RANGES;

#[derive(Debug, clap::Subcommand)]
pub enum MultiCommand {
  /// Your multireddits, or a user's public ones
  #[command(visible_alias = "multireddits")]
  Multis {
    /// User: name, `u/name` or profile link
    user: Option<String>,
  },
  /// Posts of a multireddit (`u/USER/m/NAME`, a link or `#N` after `multis`)
  #[command(after_long_help = EX_MULTI)]
  Multi {
    /// Multireddit: `u/USER/m/NAME`, `USER/NAME`, link or #N
    multireddit: String,
    /// Order of the posts
    #[arg(short, long, default_value = "hot", value_parser = SORTS)]
    sort: String,
    /// Time range of `top` and `controversial`
    #[arg(short, long, value_parser = PossibleValuesParser::new(TIME_RANGES.iter().copied()))]
    time: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
}

const EX_MULTI: &str = "\
Examples:
  atp reddit multis spez
  atp reddit multi '#1'
  atp reddit multi u/spez/m/tech --sort top --time month

More: atp guide reddit multi";

pub(super) async fn run(api: &Api, command: MultiCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    MultiCommand::Multis { user } => {
      let user = user.map(|u| ctx.user_ref(&u)).transpose()?;
      Data::Collections(multis::list(api, user.as_deref()).await?)
    }
    MultiCommand::Multi {
      multireddit,
      sort,
      time,
      page,
    } => {
      let multi = ctx.collection_ref(&multireddit)?;
      Data::Posts(
        page
          .collect_dated(async |r| multis::posts(api, &multi, &sort, time.as_deref(), &r).await)
          .await?,
      )
    }
  })
}
