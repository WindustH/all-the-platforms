//! Searching one subreddit, and posts linking to a domain.

use clap::builder::PossibleValuesParser;
use social_core::cli::PageArgs;
use social_core::{Data, Result};

use super::SORTS;
use crate::api::Api;
use crate::library;
use crate::posts::{SEARCH_SORTS, TIME_RANGES};

#[derive(Debug, clap::Subcommand)]
pub enum SearchCommand {
  /// Search inside one subreddit
  SearchIn {
    subreddit: String,
    query: String,
    #[arg(short, long, default_value = "relevance",
      value_parser = PossibleValuesParser::new(SEARCH_SORTS.iter().copied()))]
    sort: String,
    #[arg(short, long, value_parser = PossibleValuesParser::new(TIME_RANGES.iter().copied()))]
    time: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts linking to a domain
  Domain {
    domain: String,
    #[arg(short, long, default_value = "hot", value_parser = SORTS)]
    sort: String,
    #[arg(short, long, value_parser = PossibleValuesParser::new(TIME_RANGES.iter().copied()))]
    time: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub(super) async fn run(api: &Api, command: SearchCommand) -> Result<Data> {
  Ok(match command {
    SearchCommand::SearchIn {
      subreddit,
      query,
      sort,
      time,
      page,
    } => Data::Posts(
      page
        .collect_dated(async |r| {
          library::search_in(api, &subreddit, &query, &sort, time.as_deref(), &r).await
        })
        .await?,
    ),
    SearchCommand::Domain {
      domain,
      sort,
      time,
      page,
    } => Data::Posts(
      page
        .collect_dated(async |r| library::domain(api, &domain, &sort, time.as_deref(), &r).await)
        .await?,
    ),
  })
}
