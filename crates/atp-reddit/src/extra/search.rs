//! Searching one subreddit, and posts linking to a domain.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};
use clap::builder::PossibleValuesParser;

use super::SORTS;
use crate::api::Api;
use crate::library;
use crate::posts::{SEARCH_SORTS, TIME_RANGES};

#[derive(Debug, clap::Subcommand)]
pub enum SearchCommand {
  /// Search inside one subreddit
  #[command(after_long_help = EX_SEARCH_IN)]
  SearchIn {
    /// Subreddit: name, r/name or link
    subreddit: String,
    /// What to search for (Reddit's search syntax)
    query: String,
    /// Result order
    #[arg(short, long, default_value = "relevance",
      value_parser = PossibleValuesParser::new(SEARCH_SORTS.iter().copied()))]
    sort: String,
    /// Time range
    #[arg(short, long, value_parser = PossibleValuesParser::new(TIME_RANGES.iter().copied()))]
    time: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts linking to a domain
  Domain {
    /// Domain (`github.com`)
    domain: String,
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

const EX_SEARCH_IN: &str = "\
Examples:
  atp reddit search-in rust 'async traits'
  atp reddit search-in r/rust tokio --sort new
  atp reddit search-in rust tokio --sort top --time year

More: atp guide reddit search-in";

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
