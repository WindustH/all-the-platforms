//! Spaces and trends by place.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{spaces, trends};

#[derive(Debug, clap::Subcommand)]
pub enum DiscoverCommand {
  /// A Space: title, state, hosts, speakers and listeners
  Space {
    /// Space: id or link
    space: String,
  },
  /// Posts sharing a Space that match a query (newest first; `extra.space` for `space`)
  Spaces {
    /// What to search for
    query: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Trends of a place (`trend-places` lists them; 1 is worldwide)
  Trends {
    /// The place's WOEID, from `trend-places`
    #[arg(long, default_value = "1", value_name = "WOEID")]
    place: String,
  },
  /// Places with trends
  TrendPlaces,
}

pub(super) async fn run(api: &Api, command: DiscoverCommand) -> Result<Data> {
  Ok(match command {
    DiscoverCommand::Space { space } => Data::Post(Box::new(spaces::space(api, &space).await?)),
    DiscoverCommand::Spaces { query, page } => Data::Posts(
      page
        .collect_dated(async |r| spaces::search(api, &query, &r).await)
        .await?,
    ),
    DiscoverCommand::Trends { place } => Data::Posts(trends::place_trends(api, &place).await?),
    DiscoverCommand::TrendPlaces => Data::Collections(trends::places(api).await?),
  })
}
