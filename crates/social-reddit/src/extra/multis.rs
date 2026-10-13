//! Multireddits.

use clap::builder::PossibleValuesParser;
use social_core::cli::PageArgs;
use social_core::{Data, Result};

use super::SORTS;
use crate::api::Api;
use crate::multis;
use crate::posts::TIME_RANGES;

#[derive(Debug, clap::Subcommand)]
pub enum MultiCommand {
  /// Your multireddits, or a user's public ones
  #[command(visible_alias = "multireddits")]
  Multis { user: Option<String> },
  /// Posts of a multireddit (`u/USER/m/NAME`, a link or `#N` after `multis`)
  Multi {
    multireddit: String,
    #[arg(short, long, default_value = "hot", value_parser = SORTS)]
    sort: String,
    #[arg(short, long, value_parser = PossibleValuesParser::new(TIME_RANGES.iter().copied()))]
    time: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
}

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
