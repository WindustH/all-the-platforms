//! Xianyu-only commands: the ratings a user received (信用评价) and the
//! search box's suggestions.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{search, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Ratings a user received from the people they traded with
  Ratings {
    /// User id, profile link or `me`
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Words the search box suggests for a keyword
  Suggest {
    keyword: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Ratings { user, page } => Data::Comments(
      page
        .collect_dated(async |r| users::ratings(api, &user, &r).await)
        .await?,
    ),
    Command::Suggest { keyword, page } => Data::Posts(
      page
        .collect(async |r| search::suggest(api, &keyword, &r).await)
        .await?,
    ),
  })
}
