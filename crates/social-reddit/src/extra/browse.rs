//! Browsing one subreddit, and subreddit details and listings.

use clap::builder::PossibleValuesParser;
use social_core::cli::PageArgs;
use social_core::{Data, Page, Result};

use crate::api::Api;
use crate::posts::TIME_RANGES;
use crate::{posts, subreddits};

#[derive(Debug, clap::Subcommand)]
pub enum BrowseCommand {
  /// Posts of one subreddit
  #[command(visible_alias = "r")]
  Sub {
    /// Name, r/name or link
    subreddit: String,
    /// Order of the posts
    #[arg(short, long, default_value = "hot",
      value_parser = ["hot", "new", "top", "rising", "controversial"])]
    sort: String,
    /// Time range of `top` and `controversial`
    #[arg(short, long, value_parser = PossibleValuesParser::new(TIME_RANGES.iter().copied()))]
    time: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// About a subreddit: description, subscribers and active users
  Subreddit { subreddit: String },
  /// Popular, new or default subreddits
  Subreddits {
    #[arg(default_value = "popular", value_parser = ["popular", "new", "default"])]
    which: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub(super) async fn run(api: &Api, command: BrowseCommand) -> Result<Data> {
  Ok(match command {
    BrowseCommand::Sub {
      subreddit,
      sort,
      time,
      page,
    } => Data::Posts(
      page
        .collect_dated(async |r| {
          posts::subreddit(api, &subreddit, &sort, time.as_deref(), &r).await
        })
        .await?,
    ),
    BrowseCommand::Subreddit { subreddit } => {
      Data::Collections(Page::last(vec![subreddits::about(api, &subreddit).await?]))
    }
    BrowseCommand::Subreddits { which, page } => Data::Collections(
      page
        .collect(async |r| subreddits::listed(api, &which, &r).await)
        .await?,
    ),
  })
}
